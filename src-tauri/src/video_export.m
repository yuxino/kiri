#import <AVFoundation/AVFoundation.h>
#import <Foundation/Foundation.h>
#import <CoreImage/CoreImage.h>
#import <math.h>

typedef struct { double start; double end; double speed; } KiriVideoSegment;
typedef struct {
    unsigned kind;
    double start, end, x, y, width, height;
    unsigned maskStyle;
    double strength, transition;
    unsigned color;
    int layer;
} KiriVideoEffect;
typedef struct {
    unsigned kind;
    double start, end, x, y, width, height, amount;
    const unsigned char *pixels;
    unsigned pixelWidth, pixelHeight;
} KiriVideoAnnotation;
typedef bool (*KiriExportProgress)(const void *context, double progress);

static bool KiriExportContinue(KiriExportProgress progress, const void *context, double value,
                               char *error, size_t capacity) {
    if (progress && !progress(context, value)) {
        snprintf(error, capacity, "VIDEO_EXPORT_CANCELLED");
        return false;
    }
    return true;
}

static CIImage *KiriPlacedAnnotation(CIImage *image, KiriVideoAnnotation annotation, CGRect extent) {
    CGRect rect = CGRectMake(extent.origin.x + annotation.x * extent.size.width,
        extent.origin.y + (1 - annotation.y - annotation.height) * extent.size.height,
        annotation.width * extent.size.width, annotation.height * extent.size.height);
    image = [image imageByApplyingTransform:CGAffineTransformMakeScale(rect.size.width / annotation.pixelWidth, rect.size.height / annotation.pixelHeight)];
    return [image imageByApplyingTransform:CGAffineTransformMakeTranslation(rect.origin.x, rect.origin.y)];
}

static bool KiriAudioError(NSError *failure, NSString *fallback, char *error, size_t capacity) {
    snprintf(error, capacity, "%s", (failure.localizedDescription ?: fallback).UTF8String);
    return false;
}

static bool KiriHasSpeedAudioFrames(CMTimeRange range, double speed) {
    double seconds = CMTimeGetSeconds(range.duration);
    return CMTIMERANGE_IS_VALID(range) && isfinite(seconds) && seconds >= MAX(1, speed) / 96000.0;
}

// AVAssetExportSession's time-pitch mix can intermittently lose samples at rate
// boundaries. Render changed-rate audio offline first, then insert it at 1x.
// Both decoding and rendering use fixed-size buffers; long clips stay on disk.
static AVURLAsset *KiriRenderSpeedAudio(AVAsset *asset, AVAssetTrack *track,
    CMTimeRange range, double speed, NSURL *directory, double progressStart, double progressEnd,
    KiriExportProgress progress, const void *context, char *error, size_t capacity) {
    const double sampleRate = 48000;
    const AVAudioFrameCount blockSize = 4096;
    CMAudioFormatDescriptionRef description = (__bridge CMAudioFormatDescriptionRef)track.formatDescriptions.firstObject;
    const AudioStreamBasicDescription *sourceFormat = description ? CMAudioFormatDescriptionGetStreamBasicDescription(description) : NULL;
    AVAudioChannelCount channels = sourceFormat ? sourceFormat->mChannelsPerFrame : 2;
    if (channels == 0 || channels > 32) {
        KiriAudioError(nil, @"Unsupported audio channel count.", error, capacity); return nil;
    }
    AVAudioFormat *format = [[AVAudioFormat alloc] initStandardFormatWithSampleRate:sampleRate channels:channels];
    NSDictionary *settings = @{AVFormatIDKey: @(kAudioFormatLinearPCM), AVSampleRateKey: @(sampleRate),
        AVNumberOfChannelsKey: @(channels), AVLinearPCMBitDepthKey: @32,
        AVLinearPCMIsFloatKey: @YES, AVLinearPCMIsNonInterleaved: @NO};
    NSString *name = NSUUID.UUID.UUIDString;
    NSURL *inputURL = [directory URLByAppendingPathComponent:[name stringByAppendingString:@"-input.caf"]];
    NSURL *outputURL = [directory URLByAppendingPathComponent:[name stringByAppendingString:@"-output.caf"]];
    NSError *failure = nil;
    AVAssetReader *reader = [[AVAssetReader alloc] initWithAsset:asset error:&failure];
    AVAssetReaderTrackOutput *decoded = [[AVAssetReaderTrackOutput alloc] initWithTrack:track outputSettings:settings];
    decoded.alwaysCopiesSampleData = NO;
    if (!reader || ![reader canAddOutput:decoded]) {
        KiriAudioError(failure, @"Could not prepare audio decoding.", error, capacity); return nil;
    }
    [reader addOutput:decoded];
    reader.timeRange = range;
    AVAudioFile *input = [[AVAudioFile alloc] initForWriting:inputURL settings:settings error:&failure];
    if (!input) { KiriAudioError(failure, @"Could not stage audio.", error, capacity); return nil; }
    AVAudioPCMBuffer *buffer = [[AVAudioPCMBuffer alloc] initWithPCMFormat:format frameCapacity:blockSize];
    NSMutableData *interleaved = [NSMutableData dataWithLength:(size_t)blockSize * channels * sizeof(float)];
    AVAudioFramePosition wanted = llround(CMTimeGetSeconds(range.duration) * sampleRate), written = 0;
    AVAudioEngine *engine = nil;
    @try {
        if (![reader startReading]) { KiriAudioError(reader.error, @"Could not decode audio.", error, capacity); return nil; }
        CMSampleBufferRef sample;
        while (written < wanted && (sample = [decoded copyNextSampleBuffer])) {
          @autoreleasepool {
            @try {
                CMBlockBufferRef bytes = CMSampleBufferGetDataBuffer(sample);
                const AudioStreamBasicDescription *pcm = CMAudioFormatDescriptionGetStreamBasicDescription(CMSampleBufferGetFormatDescription(sample));
                if (!bytes || !pcm || pcm->mSampleRate != sampleRate || pcm->mChannelsPerFrame != channels
                    || pcm->mBytesPerFrame != channels * sizeof(float) || !(pcm->mFormatFlags & kAudioFormatFlagIsFloat)) {
                    KiriAudioError(nil, @"Could not decode the audio sample format.", error, capacity); return nil;
                }
                AVAudioFramePosition position = llround(CMTimeGetSeconds(CMTimeSubtract(CMSampleBufferGetPresentationTimeStamp(sample), range.start)) * sampleRate);
                AVAudioFramePosition count = (AVAudioFramePosition)(CMBlockBufferGetDataLength(bytes) / pcm->mBytesPerFrame);
                AVAudioFramePosition offset = MAX(0, written - position);
                // Preserve real gaps in a track, and trim AAC packet padding using
                // presentation timestamps rather than assuming contiguous packets.
                while (written < wanted && (written < position || offset < count)) {
                    if (!KiriExportContinue(progress, context,
                        progressStart + (progressEnd - progressStart) * 0.5 * written / wanted, error, capacity)) return nil;
                    bool silence = written < position;
                    AVAudioFrameCount frames = (AVAudioFrameCount)MIN(blockSize, MIN(wanted - written,
                        silence ? position - written : count - offset));
                    buffer.frameLength = frames;
                    if (!silence && CMBlockBufferCopyDataBytes(bytes, (size_t)offset * pcm->mBytesPerFrame,
                        (size_t)frames * pcm->mBytesPerFrame, interleaved.mutableBytes) != kCMBlockBufferNoErr) {
                        KiriAudioError(nil, @"Could not read decoded audio.", error, capacity); return nil;
                    }
                    const float *values = interleaved.bytes;
                    for (AVAudioChannelCount channel = 0; channel < channels; channel++) {
                        float *destination = buffer.floatChannelData[channel];
                        for (AVAudioFrameCount frame = 0; frame < frames; frame++)
                            destination[frame] = silence ? 0 : values[(size_t)frame * channels + channel];
                    }
                    if (![input writeFromBuffer:buffer error:&failure]) {
                        KiriAudioError(failure, @"Could not stage decoded audio.", error, capacity); return nil;
                    }
                    written += frames;
                    if (!silence) offset += frames;
                }
            } @finally { CFRelease(sample); }
          }
        }
        if (reader.status == AVAssetReaderStatusFailed) {
            KiriAudioError(reader.error, @"Could not decode audio.", error, capacity); return nil;
        }
        while (written < wanted) {
            if (!KiriExportContinue(progress, context,
                progressStart + (progressEnd - progressStart) * 0.5 * written / wanted, error, capacity)) return nil;
            buffer.frameLength = (AVAudioFrameCount)MIN(blockSize, wanted - written);
            for (AVAudioChannelCount channel = 0; channel < channels; channel++)
                memset(buffer.floatChannelData[channel], 0, buffer.frameLength * sizeof(float));
            if (![input writeFromBuffer:buffer error:&failure]) {
                KiriAudioError(failure, @"Could not finish staged audio.", error, capacity); return nil;
            }
            written += buffer.frameLength;
        }
        input = nil; // Close the PCM writer before opening the streaming reader.
        if (!KiriExportContinue(progress, context, (progressStart + progressEnd) * 0.5, error, capacity)) return nil;
        AVAudioFile *file = [[AVAudioFile alloc] initForReading:inputURL error:&failure];
        if (!file) { KiriAudioError(failure, @"Could not open staged audio.", error, capacity); return nil; }
        engine = [[AVAudioEngine alloc] init];
        AVAudioPlayerNode *player = [[AVAudioPlayerNode alloc] init];
        AVAudioUnitTimePitch *pitch = [[AVAudioUnitTimePitch alloc] init];
        pitch.rate = speed;
        [engine attachNode:player]; [engine attachNode:pitch];
        [engine connect:player to:pitch format:format];
        [engine connect:pitch to:engine.mainMixerNode format:format];
        // Offline mode never connects to a microphone or physical audio output.
        if (![engine enableManualRenderingMode:AVAudioEngineManualRenderingModeOffline format:format
            maximumFrameCount:blockSize error:&failure]) {
            KiriAudioError(failure, @"Could not prepare audio speed processing.", error, capacity); return nil;
        }
        [player scheduleFile:file atTime:nil completionHandler:nil];
        if (![engine startAndReturnError:&failure]) {
            KiriAudioError(failure, @"Could not start audio speed processing.", error, capacity); return nil;
        }
        [player play];
        AVAudioFile *rendered = [[AVAudioFile alloc] initForWriting:outputURL settings:settings error:&failure];
        if (!rendered) { KiriAudioError(failure, @"Could not save processed audio.", error, capacity); return nil; }
        AVAudioFramePosition renderFrames = llround(CMTimeGetSeconds(range.duration) * sampleRate / speed);
        AVAudioFramePosition remaining = renderFrames;
        double lastProgress = NSProcessInfo.processInfo.systemUptime;
        while (remaining > 0) {
          @autoreleasepool {
            if (!KiriExportContinue(progress, context, progressStart + (progressEnd - progressStart)
                * (0.5 + 0.5 * (renderFrames - remaining) / renderFrames), error, capacity)) return nil;
            AVAudioFrameCount frames = (AVAudioFrameCount)MIN(blockSize, remaining);
            AVAudioEngineManualRenderingStatus status = [engine renderOffline:frames toBuffer:buffer error:&failure];
            if ((status == AVAudioEngineManualRenderingStatusSuccess || status == AVAudioEngineManualRenderingStatusInsufficientDataFromInputNode)
                && buffer.frameLength > 0) {
                if (![rendered writeFromBuffer:buffer error:&failure]) {
                    KiriAudioError(failure, @"Could not save processed audio.", error, capacity); return nil;
                }
                remaining -= buffer.frameLength;
                lastProgress = NSProcessInfo.processInfo.systemUptime;
            } else if (status == AVAudioEngineManualRenderingStatusError
                || NSProcessInfo.processInfo.systemUptime - lastProgress > 5) {
                KiriAudioError(failure, @"Audio speed processing stalled.", error, capacity); return nil;
            }
          }
        }
        [player stop]; [engine stop];
        rendered = nil;
        [[NSFileManager defaultManager] removeItemAtURL:inputURL error:nil];
        return [AVURLAsset URLAssetWithURL:outputURL options:nil];
    } @finally {
        if (reader.status == AVAssetReaderStatusReading) [reader cancelReading];
        [engine stop];
    }
}

// Called only on a background worker; the source is immutable and output is staging.
bool kiri_export_video(const char *source, const char *output, const KiriVideoSegment *segments,
                       size_t count, const KiriVideoEffect *effects, size_t effectCount, const KiriVideoAnnotation *annotations, size_t annotationCount, const size_t *order, size_t orderCount, unsigned maxEdge,
                       KiriExportProgress progress, const void *progressContext, char *error, size_t capacity) {
    @autoreleasepool {
        NSURL *audioDirectory = nil;
        @try {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
            if (!KiriExportContinue(progress, progressContext, -1, error, capacity)) return false;
            AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:
                [[NSFileManager defaultManager] stringWithFileSystemRepresentation:source length:strlen(source)]] options:nil];
            double duration = CMTimeGetSeconds(asset.duration);
            AVAssetTrack *track = [asset tracksWithMediaType:AVMediaTypeVideo].firstObject;
            if (!track || !isfinite(duration) || duration <= 0 || !segments || count == 0 || count > 128) {
                snprintf(error, capacity, "Invalid video or segments."); return false;
            }
            AVMutableComposition *edited = [AVMutableComposition composition];
            AVMutableCompositionTrack *video = [edited addMutableTrackWithMediaType:AVMediaTypeVideo preferredTrackID:kCMPersistentTrackID_Invalid];
            video.preferredTransform = track.preferredTransform;
            NSArray<AVAssetTrack *> *audioTracks = [asset tracksWithMediaType:AVMediaTypeAudio];
            NSMutableArray<AVMutableCompositionTrack *> *audioOutputs = [NSMutableArray array];
            // AVAssetTrack.asset is weak. Keep every staged asset alive until the
            // export session has completed, including cancellation cleanup.
            __attribute__((objc_precise_lifetime)) NSMutableArray<AVURLAsset *> *audioAssets = [NSMutableArray array];
            size_t speedAudioCount = 0, completedSpeedAudio = 0;
            // Count only real audio intersections. Invalid segment requests still
            // fail below before insertion; this pass only allocates progress.
            for (size_t index = 0; index < count; index++) {
                double start = segments[index].start, end = MIN(segments[index].end, duration), speed = segments[index].speed;
                if (!isfinite(start) || !isfinite(end) || !isfinite(speed) || start < 0 || end <= start
                    || speed < 0.25 || speed > 4 || speed == 1) continue;
                CMTimeRange range = CMTimeRangeFromTimeToTime(CMTimeMakeWithSeconds(start, 600000), CMTimeMakeWithSeconds(end, 600000));
                for (AVAssetTrack *audio in audioTracks)
                    if (KiriHasSpeedAudioFrames(CMTimeRangeGetIntersection(range, audio.timeRange), speed)) speedAudioCount++;
            }
            double audioProgress = speedAudioCount > 0 ? 0.2 : 0;
            NSMutableArray<NSValue *> *outputRanges = [NSMutableArray arrayWithCapacity:count];
            CMTime cursor = kCMTimeZero;
            for (size_t index = 0; index < count; index++) {
                if (!KiriExportContinue(progress, progressContext, -1, error, capacity)) return false;
                double start = segments[index].start, end = segments[index].end, speed = segments[index].speed;
                if (!isfinite(start) || !isfinite(end) || start < 0 || !isfinite(speed) || speed < 0.25 || speed > 4 || end <= start || start >= duration || end > duration + 0.05) {
                    snprintf(error, capacity, "Invalid or overlapping video segments."); return false;
                }
                end = MIN(end, duration);
                for (size_t other = 0; other < index; other++) {
                    if (start < MIN(segments[other].end, duration) && segments[other].start < end) {
                        snprintf(error, capacity, "Overlapping video segments."); return false;
                    }
                }
                CMTimeRange range = CMTimeRangeFromTimeToTime(CMTimeMakeWithSeconds(start, 600000), CMTimeMakeWithSeconds(end, 600000));
                NSError *insertError = nil;
                if (![video insertTimeRange:range ofTrack:track atTime:cursor error:&insertError]) {
                    snprintf(error, capacity, "%s", (insertError.localizedDescription ?: @"Could not insert video segment.").UTF8String); return false;
                }
                CMTime scaledDuration = CMTimeMultiplyByFloat64(range.duration, 1.0 / speed);
                [video scaleTimeRange:CMTimeRangeMake(cursor, range.duration) toDuration:scaledDuration];
                for (NSUInteger audioIndex = 0; audioIndex < audioTracks.count; audioIndex++) {
                    AVAssetTrack *audio = audioTracks[audioIndex];
                    // Audio may begin later or end sooner than video. Preserve that offset.
                    CMTimeRange intersection = CMTimeRangeGetIntersection(range, audio.timeRange);
                    if (CMTIMERANGE_IS_VALID(intersection) && CMTimeCompare(intersection.duration, kCMTimeZero) > 0) {
                        if (speed != 1.0 && !KiriHasSpeedAudioFrames(intersection, speed)) continue;
                        AVMutableCompositionTrack *audioOutput = [edited addMutableTrackWithMediaType:AVMediaTypeAudio preferredTrackID:kCMPersistentTrackID_Invalid];
                        [audioOutputs addObject:audioOutput];
                        CMTime position = CMTimeAdd(cursor, CMTimeMultiplyByFloat64(CMTimeSubtract(intersection.start, range.start), 1.0 / speed));
                        AVAssetTrack *insertTrack = audio;
                        CMTimeRange insertRange = intersection;
                        if (speed != 1.0) {
                            if (!audioDirectory) {
                                audioDirectory = [NSURL fileURLWithPath:[NSTemporaryDirectory() stringByAppendingPathComponent:
                                    [@"kiri-video-audio-" stringByAppendingString:NSUUID.UUID.UUIDString]] isDirectory:YES];
                                if (![[NSFileManager defaultManager] createDirectoryAtURL:audioDirectory
                                    withIntermediateDirectories:NO attributes:nil error:&insertError]) {
                                    return KiriAudioError(insertError, @"Could not prepare temporary audio storage.", error, capacity);
                                }
                            }
                            AVURLAsset *processed = KiriRenderSpeedAudio(asset, audio, intersection, speed,
                                audioDirectory, audioProgress * completedSpeedAudio / speedAudioCount,
                                audioProgress * (completedSpeedAudio + 1) / speedAudioCount,
                                progress, progressContext, error, capacity);
                            if (!processed) return false;
                            completedSpeedAudio++;
                            if (!KiriExportContinue(progress, progressContext,
                                audioProgress * completedSpeedAudio / speedAudioCount, error, capacity)) return false;
                            [audioAssets addObject:processed];
                            insertTrack = [processed tracksWithMediaType:AVMediaTypeAudio].firstObject;
                            if (!insertTrack) return KiriAudioError(nil, @"Processed audio is unavailable.", error, capacity);
                            insertRange = insertTrack.timeRange;
                        }
                        if (![audioOutput insertTimeRange:insertRange ofTrack:insertTrack atTime:position error:&insertError]) {
                            snprintf(error, capacity, "%s", (insertError.localizedDescription ?: @"Could not insert audio segment.").UTF8String); return false;
                        }
                    }
                }
                [outputRanges addObject:[NSValue valueWithCMTimeRange:CMTimeRangeMake(cursor, scaledDuration)]];
                cursor = CMTimeAdd(cursor, scaledDuration);
            }
            AVAssetExportSession *session = [[AVAssetExportSession alloc]
                initWithAsset:edited presetName:AVAssetExportPresetHighestQuality];
            if (!session) { snprintf(error, capacity, "Could not create the MP4 exporter."); return false; }
            AVMutableAudioMix *audioMix = [AVMutableAudioMix audioMix];
            NSMutableArray<AVAudioMixInputParameters *> *audioParameters = [NSMutableArray array];
            for (AVMutableCompositionTrack *audio in audioOutputs) {
                AVMutableAudioMixInputParameters *parameters = [AVMutableAudioMixInputParameters audioMixInputParametersWithTrack:audio];
                parameters.audioTimePitchAlgorithm = AVAudioTimePitchAlgorithmSpectral;
                [audioParameters addObject:parameters];
            }
            audioMix.inputParameters = audioParameters;
            session.audioMix = audioMix;
            CGRect bounds = CGRectApplyAffineTransform((CGRect){CGPointZero, track.naturalSize}, track.preferredTransform);
            double width = fabs(bounds.size.width), height = fabs(bounds.size.height);
            if (width < 2 || height < 2) { snprintf(error, capacity, "Invalid video dimensions."); return false; }
            double scale = maxEdge && MAX(width, height) > maxEdge ? maxEdge / MAX(width, height) : 1.0;
            CGSize target = scale < 1 ? CGSizeMake(MAX(2, floor(width * scale / 2) * 2), MAX(2, floor(height * scale / 2) * 2)) : CGSizeMake(width, height);
            // Resizing preserves source timing. Timed edits use a separate render
            // cadence below so a held recording frame cannot suppress an effect.
            CMTime frameDuration = track.minFrameDuration;
            double frameSeconds = CMTimeGetSeconds(frameDuration);
            if (!CMTIME_IS_NUMERIC(frameDuration) || !isfinite(frameSeconds) || frameSeconds <= 0) {
                float fps = track.nominalFrameRate;
                frameDuration = CMTimeMakeWithSeconds(1.0 / (isfinite(fps) && fps > 0 ? fps : 30), 600000);
            }
            NSMutableArray<CIImage *> *annotationImages = [NSMutableArray arrayWithCapacity:annotationCount];
            for (size_t index = 0; index < annotationCount; index++) {
                if (!KiriExportContinue(progress, progressContext, -1, error, capacity)) return false;
                KiriVideoAnnotation annotation = annotations[index];
                NSData *data = [NSData dataWithBytes:annotation.pixels length:(size_t)annotation.pixelWidth * annotation.pixelHeight * 4];
                CGDataProviderRef provider = CGDataProviderCreateWithCFData((__bridge CFDataRef)data);
                CGColorSpaceRef colorSpace = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
                CGImageRef cgImage = CGImageCreate(annotation.pixelWidth, annotation.pixelHeight, 8, 32,
                    annotation.pixelWidth * 4, colorSpace, kCGImageAlphaLast | kCGBitmapByteOrderDefault,
                    provider, NULL, false, kCGRenderingIntentDefault);
                CIImage *image = cgImage ? [CIImage imageWithCGImage:cgImage] : nil;
                if (cgImage) CGImageRelease(cgImage);
                CGColorSpaceRelease(colorSpace);
                CGDataProviderRelease(provider);
                if (!image) { snprintf(error, capacity, "Could not prepare video annotation image."); return false; }
                [annotationImages addObject:image];
            }
            if (effectCount > 0 || annotationCount > 0) {
                // Canvas and the Windows compositor blend in sRGB, not linear light.
                CGColorSpaceRef workingSpace = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
                CIContext *effectContext = [CIContext contextWithOptions:@{kCIContextWorkingColorSpace: (__bridge id)workingSpace}];
                CGColorSpaceRelease(workingSpace);
                AVVideoComposition *filtered = [AVVideoComposition videoCompositionWithAsset:edited applyingCIFiltersWithHandler:^(AVAsynchronousCIImageFilteringRequest *request) {
                    double sourceTime = segments[count - 1].end;
                    for (size_t index = 0; index < count; index++) {
                        // Match the exact composition clock. Accumulating doubles
                        // can classify the first frame after a fractional cut as
                        // the previous clip, leaving its privacy mask unapplied.
                        CMTimeRange outputRange = outputRanges[index].CMTimeRangeValue;
                        if (CMTimeCompare(request.compositionTime, CMTimeRangeGetEnd(outputRange)) < 0) {
                            CMTime elapsed = CMTimeSubtract(request.compositionTime, outputRange.start);
                            CMTime sourceStart = CMTimeMakeWithSeconds(segments[index].start, 600000);
                            sourceTime = CMTimeGetSeconds(CMTimeAdd(sourceStart,
                                CMTimeMultiplyByFloat64(elapsed, segments[index].speed)));
                            break;
                        }
                    }
                    CIImage *image = request.sourceImage;
                    CGRect extent = image.extent;
                    // CI coordinates are bottom-left; UI rectangles are normalized top-left.
                    // Visit one layer at a time in the explicit timeline order.
                    for (size_t position = 0; position < orderCount; position++) {
                        size_t item = order[position];
                    for (size_t index = item; index < MIN(item + 1, annotationCount); index++) {
                        KiriVideoAnnotation annotation = annotations[index];
                        if (annotation.kind == 0 || sourceTime < annotation.start || sourceTime >= annotation.end) continue;
                        CIImage *mask = KiriPlacedAnnotation(annotationImages[index], annotation, extent);
                        CIImage *transparent = [[CIImage imageWithColor:[CIColor colorWithRed:0 green:0 blue:0 alpha:0]] imageByCroppingToRect:extent];
                        mask = [mask imageByCompositingOverImage:transparent];
                        double amount = MAX(1, annotation.amount * extent.size.width);
                        NSString *filter = annotation.kind == 1 ? @"CIPixellate" : @"CIGaussianBlur";
                        NSString *parameter = annotation.kind == 1 ? kCIInputScaleKey : kCIInputRadiusKey;
                        CIImage *processed = [[[image imageByClampingToExtent] imageByApplyingFilter:filter
                            withInputParameters:@{parameter: @(amount)}] imageByCroppingToRect:extent];
                        image = [processed imageByApplyingFilter:@"CIBlendWithAlphaMask" withInputParameters:@{
                            kCIInputBackgroundImageKey: image, kCIInputMaskImageKey: mask}];
                    }
                    for (size_t index = item; index < MIN(item + 1, annotationCount); index++) {
                        KiriVideoAnnotation annotation = annotations[index];
                        if (annotation.kind != 0 || sourceTime < annotation.start || sourceTime >= annotation.end) continue;
                        CIImage *overlay = KiriPlacedAnnotation(annotationImages[index], annotation, extent);
                        image = [overlay imageByCompositingOverImage:image];
                    }
                    for (size_t index = item >= annotationCount ? item - annotationCount : effectCount; index < effectCount && index == item - annotationCount; index++) {
                        KiriVideoEffect effect = effects[index];
                        if (effect.kind != 1 || sourceTime < effect.start || sourceTime >= effect.end) continue;
                        CGRect mask = CGRectMake(extent.origin.x + effect.x * extent.size.width,
                            extent.origin.y + (1 - effect.y - effect.height) * extent.size.height,
                            effect.width * extent.size.width, effect.height * extent.size.height);
                        if (effect.maskStyle == 0) {
                            CIColor *color = [CIColor colorWithRed:((effect.color >> 16) & 255) / 255.0
                                green:((effect.color >> 8) & 255) / 255.0 blue:(effect.color & 255) / 255.0 alpha:1];
                            CIImage *cover = [[CIImage imageWithColor:color] imageByCroppingToRect:CGRectIntegral(mask)];
                            image = [cover imageByCompositingOverImage:image];
                        } else {
                            double amount = effect.maskStyle == 1
                                ? MAX(1, extent.size.width * (0.003 + 0.027 * effect.strength))
                                : MAX(2, extent.size.width * (0.005 + 0.045 * effect.strength));
                            NSString *filter = effect.maskStyle == 1 ? @"CIGaussianBlur" : @"CIPixellate";
                            NSString *parameter = effect.maskStyle == 1 ? kCIInputRadiusKey : kCIInputScaleKey;
                            CIImage *processed = [[image imageByClampingToExtent] imageByApplyingFilter:filter
                                withInputParameters:@{parameter: @(amount)}];
                            processed = [processed imageByCroppingToRect:CGRectIntegral(mask)];
                            image = [processed imageByCompositingOverImage:image];
                        }
                    }
                    for (size_t index = item >= annotationCount ? item - annotationCount : effectCount; index < effectCount && index == item - annotationCount; index++) {
                        KiriVideoEffect effect = effects[index];
                        if (effect.kind != 2 || sourceTime < effect.start || sourceTime >= effect.end) continue;
                        CGRect focus = CGRectIntegral(CGRectMake(extent.origin.x + effect.x * extent.size.width,
                            extent.origin.y + (1 - effect.y - effect.height) * extent.size.height,
                            effect.width * extent.size.width, effect.height * extent.size.height));
                        CIImage *original = [image imageByCroppingToRect:focus];
                        CIImage *shade = [[CIImage imageWithColor:[CIColor colorWithRed:0 green:0 blue:0 alpha:effect.strength]] imageByCroppingToRect:extent];
                        image = [original imageByCompositingOverImage:[shade imageByCompositingOverImage:image]];
                    }
                    for (size_t index = item >= annotationCount ? item - annotationCount : effectCount; index < effectCount && index == item - annotationCount; index++) {
                        KiriVideoEffect effect = effects[index];
                        if (effect.kind != 0 || sourceTime < effect.start || sourceTime >= effect.end) continue;
                        double ease = 1;
                        double ramp = MIN(effect.transition, (effect.end - effect.start) / 2);
                        if (ramp > 0) {
                            double progress = MIN(MIN(1, MAX(0, (sourceTime - effect.start) / ramp)),
                                MIN(1, MAX(0, (effect.end - sourceTime) / ramp)));
                            ease = progress * progress * (3 - 2 * progress);
                        }
                        double x = effect.x * ease, y = effect.y * ease;
                        double cropWidth = 1 + (effect.width - 1) * ease;
                        double cropHeight = 1 + (effect.height - 1) * ease;
                        CGRect crop = CGRectMake(extent.origin.x + x * extent.size.width,
                            extent.origin.y + (1 - y - cropHeight) * extent.size.height,
                            cropWidth * extent.size.width, cropHeight * extent.size.height);
                        image = [image imageByCroppingToRect:crop];
                        image = [image imageByApplyingTransform:CGAffineTransformMakeTranslation(-crop.origin.x, -crop.origin.y)];
                        image = [image imageByApplyingTransform:CGAffineTransformMakeScale(extent.size.width / crop.size.width, extent.size.height / crop.size.height)];
                        image = [image imageByApplyingTransform:CGAffineTransformMakeTranslation(extent.origin.x, extent.origin.y)];
                    }
                    for (size_t index = item >= annotationCount ? item - annotationCount : effectCount; index < effectCount && index == item - annotationCount; index++) {
                        KiriVideoEffect effect = effects[index];
                        if (sourceTime < effect.start || sourceTime >= effect.end) continue;
                        if (effect.kind == 3) {
                            double padding = effect.strength * 0.25;
                            double scale = MIN((1 - 2 * padding) / effect.width, (1 - 2 * padding) / effect.height);
                            CGRect crop = CGRectMake(extent.origin.x + effect.x * extent.size.width,
                                extent.origin.y + (1 - effect.y - effect.height) * extent.size.height,
                                effect.width * extent.size.width, effect.height * extent.size.height);
                            CIImage *content = [[image imageByCroppingToRect:crop] imageByApplyingTransform:CGAffineTransformMakeTranslation(-crop.origin.x, -crop.origin.y)];
                            content = [content imageByApplyingTransform:CGAffineTransformMakeScale(scale, scale)];
                            content = [content imageByApplyingTransform:CGAffineTransformMakeTranslation(extent.origin.x + (extent.size.width - crop.size.width * scale) / 2,
                                extent.origin.y + (extent.size.height - crop.size.height * scale) / 2)];
                            CIColor *color = [CIColor colorWithRed:((effect.color >> 16) & 255) / 255.0
                                green:((effect.color >> 8) & 255) / 255.0 blue:(effect.color & 255) / 255.0 alpha:1];
                            image = [content imageByCompositingOverImage:[[CIImage imageWithColor:color] imageByCroppingToRect:extent]];
                        }
                    }
                    for (size_t index = item >= annotationCount ? item - annotationCount : effectCount; index < effectCount && index == item - annotationCount; index++) {
                        KiriVideoEffect effect = effects[index];
                        if (effect.kind != 4 || sourceTime < effect.start || sourceTime >= effect.end) continue;
                        double ramp = MIN(effect.transition, (effect.end - effect.start) / 2);
                        double progress = ramp > 0 ? MIN(MIN(1, MAX(0, (sourceTime - effect.start) / ramp)), MIN(1, MAX(0, (effect.end - sourceTime) / ramp))) : 1;
                        double alpha = 1 - progress * progress * (3 - 2 * progress);
                        CIColor *color = [CIColor colorWithRed:((effect.color >> 16) & 255) / 255.0
                            green:((effect.color >> 8) & 255) / 255.0 blue:(effect.color & 255) / 255.0 alpha:alpha];
                        image = [[[CIImage imageWithColor:color] imageByCroppingToRect:extent] imageByCompositingOverImage:image];
                    }
                    }
                    image = [image imageByCroppingToRect:extent];
                    if (target.width != extent.size.width || target.height != extent.size.height) {
                        image = [image imageByApplyingTransform:CGAffineTransformMakeScale(target.width / extent.size.width, target.height / extent.size.height)];
                    }
                    [request finishWithImage:image context:effectContext];
                }];
                AVMutableVideoComposition *composition = [filtered mutableCopy];
                // The CI factory follows the source track even when frameDuration
                // is set. Disable that override: VFR recordings can hold one image
                // across an entire mask, annotation, zoom or fade interval.
                composition.sourceTrackIDForFrameTiming = kCMPersistentTrackID_Invalid;
                // A single short VFR interval (Simulator can report 1/600 s) is not
                // a useful output rate. Keep ordinary/high-rate footage up to 120
                // fps, with at least 30 fps for smooth effects on sparse recordings.
                double effectFPS = track.nominalFrameRate;
                if (!isfinite(effectFPS) || effectFPS <= 0) effectFPS = 30;
                composition.frameDuration = CMTimeMakeWithSeconds(1.0 / MAX(30, MIN(120, effectFPS)), 600000);
                composition.renderSize = target;
                session.videoComposition = composition;
            } else if (scale < 1) {
                AVMutableVideoComposition *composition = [AVMutableVideoComposition videoComposition];
                composition.renderSize = target;
                composition.frameDuration = frameDuration;
                composition.sourceTrackIDForFrameTiming = video.trackID;
                AVMutableVideoCompositionInstruction *instruction = [AVMutableVideoCompositionInstruction videoCompositionInstruction];
                instruction.timeRange = CMTimeRangeMake(kCMTimeZero, cursor);
                AVMutableVideoCompositionLayerInstruction *layer = [AVMutableVideoCompositionLayerInstruction videoCompositionLayerInstructionWithAssetTrack:video];
                CGAffineTransform transform = CGAffineTransformConcat(track.preferredTransform,
                    CGAffineTransformMakeTranslation(-bounds.origin.x, -bounds.origin.y));
                transform = CGAffineTransformConcat(transform, CGAffineTransformMakeScale(target.width / width, target.height / height));
                [layer setTransform:transform atTime:kCMTimeZero];
                instruction.layerInstructions = @[layer];
                composition.instructions = @[instruction];
                session.videoComposition = composition;
            }
            session.timeRange = CMTimeRangeMake(kCMTimeZero, cursor);
            session.outputURL = [NSURL fileURLWithPath:[[NSFileManager defaultManager]
                stringWithFileSystemRepresentation:output length:strlen(output)]];
            session.outputFileType = AVFileTypeMPEG4;
            session.shouldOptimizeForNetworkUse = YES;
            dispatch_semaphore_t done = dispatch_semaphore_create(0);
            if (!KiriExportContinue(progress, progressContext, audioProgress, error, capacity)) return false;
            [session exportAsynchronouslyWithCompletionHandler:^{ dispatch_semaphore_signal(done); }];
            double deadline = NSProcessInfo.processInfo.systemUptime + 3600;
            bool cancelled = false, timedOut = false;
            while (dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 100LL * NSEC_PER_MSEC))) {
                if (!cancelled && !timedOut) {
                    cancelled = !KiriExportContinue(progress, progressContext,
                        audioProgress + (1 - audioProgress) * session.progress, error, capacity);
                    timedOut = NSProcessInfo.processInfo.systemUptime >= deadline;
                    if (cancelled || timedOut) [session cancelExport];
                }
            }
            // The completion handler owns the native resources until it fires. Do
            // not return on cancellation before the output handle has been released.
            if (cancelled) return false;
            if (timedOut) { snprintf(error, capacity, "MP4 export timed out."); return false; }
            if (!KiriExportContinue(progress, progressContext,
                audioProgress + (1 - audioProgress) * session.progress, error, capacity)) return false;
            if (session.status != AVAssetExportSessionStatusCompleted) {
                snprintf(error, capacity, "%s", (session.error.localizedDescription ?: @"MP4 export failed.").UTF8String);
                return false;
            }
#pragma clang diagnostic pop
            return true;
        } @catch (NSException *exception) {
            snprintf(error, capacity, "%s", (exception.reason ?: @"MP4 export failed.").UTF8String);
            return false;
        } @finally {
            if (audioDirectory) [[NSFileManager defaultManager] removeItemAtURL:audioDirectory error:nil];
        }
    }
}

// Read-only native inspection used by isolated media fixture tests.
bool kiri_test_video_audio_stats(const char *path, double probeStart, double probeEnd,
                                  double *start, double *duration, double *frequency) {
    @autoreleasepool {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:
            [[NSFileManager defaultManager] stringWithFileSystemRepresentation:path length:strlen(path)]] options:nil];
        AVAssetTrack *audio = [asset tracksWithMediaType:AVMediaTypeAudio].firstObject;
        if (!audio) return false;
        *start = CMTimeGetSeconds(audio.timeRange.start);
        *duration = CMTimeGetSeconds(audio.timeRange.duration);
        AVAssetReader *reader = [[AVAssetReader alloc] initWithAsset:asset error:nil];
        AVAssetReaderTrackOutput *output = [[AVAssetReaderTrackOutput alloc] initWithTrack:audio outputSettings:@{
            AVFormatIDKey: @(kAudioFormatLinearPCM), AVSampleRateKey: @48000,
            AVNumberOfChannelsKey: @1, AVLinearPCMBitDepthKey: @32,
            AVLinearPCMIsFloatKey: @YES, AVLinearPCMIsNonInterleaved: @NO}];
        [reader addOutput:output];
        // Decode continuously so an AAC seek cannot add decoder priming silence
        // to the frequency measurement. Select the probe by sample timestamps.
        if (![reader startReading]) return false;
        size_t frames = 0, crossings = 0;
        float previous = 0, peak = 0;
        CMSampleBufferRef sample;
        while ((sample = [output copyNextSampleBuffer])) {
            CMBlockBufferRef buffer = CMSampleBufferGetDataBuffer(sample);
            size_t length = buffer ? CMBlockBufferGetDataLength(buffer) : 0;
            NSMutableData *bytes = [NSMutableData dataWithLength:length];
            if (!buffer || CMBlockBufferCopyDataBytes(buffer, 0, length, bytes.mutableBytes) != kCMBlockBufferNoErr) {
                CFRelease(sample); return false;
            }
            const float *values = bytes.bytes;
            double sampleStart = CMTimeGetSeconds(CMSampleBufferGetPresentationTimeStamp(sample));
            for (size_t index = 0; index < length / sizeof(float); index++) {
                double time = sampleStart + (double)index / 48000;
                if (time < probeStart || time >= probeEnd) continue;
                if (previous <= 0 && values[index] > 0) crossings++;
                previous = values[index]; peak = MAX(peak, fabsf(previous)); frames++;
            }
            CFRelease(sample);
        }
        *frequency = frames && peak > 0.005 ? (double)crossings * 48000 / frames : 0;
        return reader.status == AVAssetReaderStatusCompleted && frames > 0;
#pragma clang diagnostic pop
    }
}

// Fixture helper: delayed and truncated audio is intentionally independent of video.
bool kiri_test_video_delayed_audio(const char *source, const char *destination) {
    @autoreleasepool {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:
            [[NSFileManager defaultManager] stringWithFileSystemRepresentation:source length:strlen(source)]] options:nil];
        AVMutableComposition *composition = [AVMutableComposition composition];
        AVMutableCompositionTrack *video = [composition addMutableTrackWithMediaType:AVMediaTypeVideo preferredTrackID:kCMPersistentTrackID_Invalid];
        AVMutableCompositionTrack *audio = [composition addMutableTrackWithMediaType:AVMediaTypeAudio preferredTrackID:kCMPersistentTrackID_Invalid];
        if (![video insertTimeRange:CMTimeRangeMake(kCMTimeZero, asset.duration)
            ofTrack:[asset tracksWithMediaType:AVMediaTypeVideo].firstObject atTime:kCMTimeZero error:nil]) return false;
        if (![audio insertTimeRange:CMTimeRangeMake(kCMTimeZero, CMTimeMake(3,2))
            ofTrack:[asset tracksWithMediaType:AVMediaTypeAudio].firstObject atTime:CMTimeMake(1,2) error:nil]) return false;
        AVAssetExportSession *session = [[AVAssetExportSession alloc] initWithAsset:composition presetName:AVAssetExportPresetPassthrough];
        session.outputURL = [NSURL fileURLWithPath:[[NSFileManager defaultManager]
            stringWithFileSystemRepresentation:destination length:strlen(destination)]];
        session.outputFileType = AVFileTypeMPEG4;
        dispatch_semaphore_t done = dispatch_semaphore_create(0);
        [session exportAsynchronouslyWithCompletionHandler:^{ dispatch_semaphore_signal(done); }];
        dispatch_semaphore_wait(done, DISPATCH_TIME_FOREVER);
        return session.status == AVAssetExportSessionStatusCompleted;
#pragma clang diagnostic pop
    }
}

// Two independently timed audio tracks catch accidentally decoding the first
// track for every segment. This remains an isolated native media test fixture.
bool kiri_test_video_two_audio_tracks(const char *source, const char *destination) {
    @autoreleasepool {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:[NSString stringWithUTF8String:source]] options:nil];
        AVMutableComposition *composition = [AVMutableComposition composition];
        AVMutableCompositionTrack *video = [composition addMutableTrackWithMediaType:AVMediaTypeVideo preferredTrackID:kCMPersistentTrackID_Invalid];
        if (![video insertTimeRange:CMTimeRangeMake(kCMTimeZero, asset.duration)
            ofTrack:[asset tracksWithMediaType:AVMediaTypeVideo].firstObject atTime:kCMTimeZero error:nil]) return false;
        for (int index = 0; index < 2; index++) {
            AVMutableCompositionTrack *audio = [composition addMutableTrackWithMediaType:AVMediaTypeAudio preferredTrackID:kCMPersistentTrackID_Invalid];
            CMTime start = CMTimeMake(index, 1);
            if (![audio insertTimeRange:CMTimeRangeMake(start, CMTimeMake(1, 1))
                ofTrack:[asset tracksWithMediaType:AVMediaTypeAudio].firstObject atTime:start error:nil]) return false;
        }
        AVAssetExportSession *session = [[AVAssetExportSession alloc] initWithAsset:composition presetName:AVAssetExportPresetPassthrough];
        session.outputURL = [NSURL fileURLWithPath:[NSString stringWithUTF8String:destination]];
        session.outputFileType = AVFileTypeMPEG4;
        dispatch_semaphore_t done = dispatch_semaphore_create(0);
        [session exportAsynchronouslyWithCompletionHandler:^{ dispatch_semaphore_signal(done); }];
        dispatch_semaphore_wait(done, DISPATCH_TIME_FOREVER);
        AVURLAsset *result = [AVURLAsset URLAssetWithURL:session.outputURL options:nil];
        return session.status == AVAssetExportSessionStatusCompleted && [result tracksWithMediaType:AVMediaTypeAudio].count == 2;
#pragma clang diagnostic pop
    }
}

bool kiri_test_export_cancel_audio(const char *source, const char *destination,
    KiriExportProgress progress, const void *context, char *error, size_t capacity) {
    const KiriVideoSegment segment = {0, 3, 0.25};
    return kiri_export_video(source, destination, &segment, 1, NULL, 0, NULL, 0, NULL, 0,
        0, progress, context, error, capacity);
}
