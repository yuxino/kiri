#import <AVFoundation/AVFoundation.h>
#import <CoreGraphics/CoreGraphics.h>
#import <CoreMedia/CoreMedia.h>
#import <CoreVideo/CoreVideo.h>
#import <Foundation/Foundation.h>
#import <ImageIO/ImageIO.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>

#include <math.h>
#include <stdbool.h>
#include <string.h>
#include <unistd.h>

static void kiri_write_error(char *buffer, size_t capacity, NSString *message) {
    if (buffer == NULL || capacity == 0) {
        return;
    }
    const char *utf8 = message.length > 0 ? message.UTF8String : "Unknown native media error";
    if (utf8 == NULL) {
        utf8 = "Unknown native media error";
    }
    strlcpy(buffer, utf8, capacity);
}

static NSString *kiri_path(const char *path) {
    return path == NULL ? nil : [NSString stringWithUTF8String:path];
}

static NSString *kiri_writer_error(AVAssetWriter *writer, NSString *fallback) {
    return writer.error.localizedDescription ?: fallback;
}

static BOOL kiri_wait_until_ready(AVAssetWriterInput *input) {
    for (NSUInteger attempt = 0; attempt < 100; attempt += 1) {
        if (input.readyForMoreMediaData) {
            return YES;
        }
        usleep(1000);
    }
    return input.readyForMoreMediaData;
}

@interface KiriMacEncoder : NSObject
@property(nonatomic, strong) AVAssetWriter *writer;
@property(nonatomic, strong) AVAssetWriterInput *videoInput;
@property(nonatomic, strong) AVAssetWriterInputPixelBufferAdaptor *videoAdaptor;
@property(nonatomic, strong, nullable) AVAssetWriterInput *audioInput;
@property(nonatomic, assign) CMFormatDescriptionRef audioFormat;
@property(nonatomic, assign) uint32_t width;
@property(nonatomic, assign) uint32_t height;
@property(nonatomic, assign) uint32_t fps;
@property(nonatomic, assign) int64_t audioFrameIndex;
@end

@implementation KiriMacEncoder
- (void)dealloc {
    if (_audioFormat != NULL) {
        CFRelease(_audioFormat);
    }
}
@end

void *kiri_macos_encoder_create(
    const char *path,
    uint32_t width,
    uint32_t height,
    uint32_t fps,
    int64_t bitrate,
    bool audioEnabled,
    char *errorBuffer,
    size_t errorCapacity
) {
    @autoreleasepool {
        @try {
            NSString *outputPath = kiri_path(path);
            if (outputPath.length == 0 || width == 0 || height == 0 || fps == 0 || bitrate <= 0) {
                kiri_write_error(errorBuffer, errorCapacity, @"The native encoder configuration is invalid.");
                return NULL;
            }
            NSURL *outputURL = [NSURL fileURLWithPath:outputPath];
            [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];

            NSError *writerError = nil;
            AVAssetWriter *writer = [[AVAssetWriter alloc] initWithURL:outputURL
                                                               fileType:AVFileTypeMPEG4
                                                                  error:&writerError];
            if (writer == nil) {
                kiri_write_error(errorBuffer, errorCapacity, writerError.localizedDescription);
                return NULL;
            }
            writer.shouldOptimizeForNetworkUse = YES;

            NSDictionary *compression = @{
                AVVideoAverageBitRateKey: @(bitrate),
                AVVideoExpectedSourceFrameRateKey: @(fps),
                AVVideoMaxKeyFrameIntervalKey: @(fps * 2),
                AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel,
                AVVideoAllowFrameReorderingKey: @NO,
            };
            NSDictionary *videoSettings = @{
                AVVideoCodecKey: AVVideoCodecTypeH264,
                AVVideoWidthKey: @(width),
                AVVideoHeightKey: @(height),
                AVVideoCompressionPropertiesKey: compression,
            };
            AVAssetWriterInput *videoInput = [AVAssetWriterInput
                assetWriterInputWithMediaType:AVMediaTypeVideo
                outputSettings:videoSettings];
            videoInput.expectsMediaDataInRealTime = YES;
            if (![writer canAddInput:videoInput]) {
                kiri_write_error(errorBuffer, errorCapacity, @"AVAssetWriter rejected the video track.");
                return NULL;
            }
            [writer addInput:videoInput];

            NSDictionary *pixelAttributes = @{
                (NSString *)kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_32BGRA),
                (NSString *)kCVPixelBufferWidthKey: @(width),
                (NSString *)kCVPixelBufferHeightKey: @(height),
                (NSString *)kCVPixelBufferIOSurfacePropertiesKey: @{},
            };
            AVAssetWriterInputPixelBufferAdaptor *adaptor =
                [AVAssetWriterInputPixelBufferAdaptor
                    assetWriterInputPixelBufferAdaptorWithAssetWriterInput:videoInput
                    sourcePixelBufferAttributes:pixelAttributes];

            AVAssetWriterInput *audioInput = nil;
            CMFormatDescriptionRef audioFormat = NULL;
            if (audioEnabled) {
                NSDictionary *audioSettings = @{
                    AVFormatIDKey: @(kAudioFormatMPEG4AAC),
                    AVSampleRateKey: @48000,
                    AVNumberOfChannelsKey: @2,
                    AVEncoderBitRateKey: @192000,
                };
                audioInput = [AVAssetWriterInput assetWriterInputWithMediaType:AVMediaTypeAudio
                                                                 outputSettings:audioSettings];
                audioInput.expectsMediaDataInRealTime = YES;
                if (![writer canAddInput:audioInput]) {
                    kiri_write_error(errorBuffer, errorCapacity, @"AVAssetWriter rejected the audio track.");
                    return NULL;
                }
                [writer addInput:audioInput];

                AudioStreamBasicDescription description = {0};
                description.mSampleRate = 48000;
                description.mFormatID = kAudioFormatLinearPCM;
                description.mFormatFlags = kAudioFormatFlagIsSignedInteger | kAudioFormatFlagIsPacked;
                description.mBytesPerPacket = 4;
                description.mFramesPerPacket = 1;
                description.mBytesPerFrame = 4;
                description.mChannelsPerFrame = 2;
                description.mBitsPerChannel = 16;
                OSStatus formatStatus = CMAudioFormatDescriptionCreate(
                    kCFAllocatorDefault,
                    &description,
                    0,
                    NULL,
                    0,
                    NULL,
                    NULL,
                    &audioFormat
                );
                if (formatStatus != noErr || audioFormat == NULL) {
                    kiri_write_error(errorBuffer, errorCapacity, @"Could not create the PCM audio format.");
                    return NULL;
                }
            }

            if (![writer startWriting]) {
                if (audioFormat != NULL) {
                    CFRelease(audioFormat);
                }
                kiri_write_error(
                    errorBuffer,
                    errorCapacity,
                    kiri_writer_error(writer, @"AVAssetWriter could not start.")
                );
                return NULL;
            }
            [writer startSessionAtSourceTime:kCMTimeZero];

            KiriMacEncoder *encoder = [KiriMacEncoder new];
            encoder.writer = writer;
            encoder.videoInput = videoInput;
            encoder.videoAdaptor = adaptor;
            encoder.audioInput = audioInput;
            encoder.audioFormat = audioFormat;
            encoder.width = width;
            encoder.height = height;
            encoder.fps = fps;
            encoder.audioFrameIndex = 0;
            return (__bridge_retained void *)encoder;
        } @catch (NSException *exception) {
            kiri_write_error(errorBuffer, errorCapacity, exception.reason);
            return NULL;
        }
    }
}

int kiri_macos_encoder_append_video(
    void *rawEncoder,
    const uint8_t *bytes,
    size_t length,
    int64_t frameIndex,
    char *errorBuffer,
    size_t errorCapacity
) {
    @autoreleasepool {
        @try {
            KiriMacEncoder *encoder = (__bridge KiriMacEncoder *)rawEncoder;
            size_t rowBytes = (size_t)encoder.width * 4;
            size_t expected = rowBytes * (size_t)encoder.height;
            if (encoder == nil || bytes == NULL || length != expected || frameIndex < 0) {
                kiri_write_error(errorBuffer, errorCapacity, @"The native video frame is invalid.");
                return -1;
            }
            if (!kiri_wait_until_ready(encoder.videoInput)) {
                if (encoder.writer.status == AVAssetWriterStatusFailed) {
                    kiri_write_error(
                        errorBuffer,
                        errorCapacity,
                        kiri_writer_error(encoder.writer, @"AVAssetWriter stopped accepting video.")
                    );
                    return -1;
                }
                return 0;
            }
            CVPixelBufferPoolRef pool = encoder.videoAdaptor.pixelBufferPool;
            CVPixelBufferRef pixelBuffer = NULL;
            CVReturn createStatus = pool == NULL
                ? kCVReturnInvalidArgument
                : CVPixelBufferPoolCreatePixelBuffer(kCFAllocatorDefault, pool, &pixelBuffer);
            if (createStatus != kCVReturnSuccess || pixelBuffer == NULL) {
                kiri_write_error(errorBuffer, errorCapacity, @"Could not allocate a native video frame.");
                return -1;
            }
            CVPixelBufferLockBaseAddress(pixelBuffer, 0);
            uint8_t *destination = CVPixelBufferGetBaseAddress(pixelBuffer);
            size_t destinationStride = CVPixelBufferGetBytesPerRow(pixelBuffer);
            if (destination == NULL || destinationStride < rowBytes) {
                CVPixelBufferUnlockBaseAddress(pixelBuffer, 0);
                CFRelease(pixelBuffer);
                kiri_write_error(errorBuffer, errorCapacity, @"The native video buffer layout is invalid.");
                return -1;
            }
            for (uint32_t row = 0; row < encoder.height; row += 1) {
                memcpy(destination + row * destinationStride, bytes + row * rowBytes, rowBytes);
            }
            CVPixelBufferUnlockBaseAddress(pixelBuffer, 0);
            CMTime timestamp = CMTimeMake(frameIndex, encoder.fps);
            BOOL appended = [encoder.videoAdaptor appendPixelBuffer:pixelBuffer
                                                withPresentationTime:timestamp];
            CFRelease(pixelBuffer);
            if (!appended) {
                kiri_write_error(
                    errorBuffer,
                    errorCapacity,
                    kiri_writer_error(encoder.writer, @"AVAssetWriter rejected a video frame.")
                );
                return -1;
            }
            return 1;
        } @catch (NSException *exception) {
            kiri_write_error(errorBuffer, errorCapacity, exception.reason);
            return -1;
        }
    }
}

bool kiri_macos_encoder_append_audio(
    void *rawEncoder,
    const uint8_t *bytes,
    size_t length,
    char *errorBuffer,
    size_t errorCapacity
) {
    @autoreleasepool {
        @try {
            KiriMacEncoder *encoder = (__bridge KiriMacEncoder *)rawEncoder;
            if (encoder == nil || bytes == NULL || length == 0 || length % 4 != 0) {
                kiri_write_error(errorBuffer, errorCapacity, @"The native audio buffer is invalid.");
                return false;
            }
            if (encoder.audioInput == nil || encoder.audioFormat == NULL) {
                kiri_write_error(errorBuffer, errorCapacity, @"The native audio track is unavailable.");
                return false;
            }
            if (!kiri_wait_until_ready(encoder.audioInput)) {
                kiri_write_error(
                    errorBuffer,
                    errorCapacity,
                    kiri_writer_error(encoder.writer, @"AVAssetWriter stopped accepting audio.")
                );
                return false;
            }
            CMBlockBufferRef block = NULL;
            OSStatus blockStatus = CMBlockBufferCreateWithMemoryBlock(
                kCFAllocatorDefault,
                NULL,
                length,
                kCFAllocatorDefault,
                NULL,
                0,
                length,
                0,
                &block
            );
            if (blockStatus != kCMBlockBufferNoErr || block == NULL
                || CMBlockBufferReplaceDataBytes(bytes, block, 0, length) != kCMBlockBufferNoErr) {
                if (block != NULL) {
                    CFRelease(block);
                }
                kiri_write_error(errorBuffer, errorCapacity, @"Could not prepare the native audio bytes.");
                return false;
            }

            CMItemCount frameCount = (CMItemCount)(length / 4);
            CMSampleTimingInfo timing = {
                .duration = CMTimeMake(1, 48000),
                .presentationTimeStamp = CMTimeMake(encoder.audioFrameIndex, 48000),
                .decodeTimeStamp = kCMTimeInvalid,
            };
            CMSampleBufferRef sample = NULL;
            OSStatus sampleStatus = CMSampleBufferCreateReady(
                kCFAllocatorDefault,
                block,
                encoder.audioFormat,
                frameCount,
                1,
                &timing,
                0,
                NULL,
                &sample
            );
            CFRelease(block);
            if (sampleStatus != noErr || sample == NULL) {
                kiri_write_error(errorBuffer, errorCapacity, @"Could not create the native audio sample.");
                return false;
            }
            BOOL appended = [encoder.audioInput appendSampleBuffer:sample];
            CFRelease(sample);
            if (!appended) {
                kiri_write_error(
                    errorBuffer,
                    errorCapacity,
                    kiri_writer_error(encoder.writer, @"AVAssetWriter rejected an audio sample.")
                );
                return false;
            }
            encoder.audioFrameIndex += frameCount;
            return true;
        } @catch (NSException *exception) {
            kiri_write_error(errorBuffer, errorCapacity, exception.reason);
            return false;
        }
    }
}

bool kiri_macos_encoder_finish(
    void *rawEncoder,
    char *errorBuffer,
    size_t errorCapacity
) {
    @autoreleasepool {
        @try {
            KiriMacEncoder *encoder = (__bridge KiriMacEncoder *)rawEncoder;
            if (encoder == nil) {
                kiri_write_error(errorBuffer, errorCapacity, @"The native encoder is unavailable.");
                return false;
            }
            [encoder.videoInput markAsFinished];
            [encoder.audioInput markAsFinished];
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
            BOOL finished = [encoder.writer finishWriting];
#pragma clang diagnostic pop
            if (!finished || encoder.writer.status != AVAssetWriterStatusCompleted) {
                kiri_write_error(
                    errorBuffer,
                    errorCapacity,
                    kiri_writer_error(encoder.writer, @"AVAssetWriter could not finalize the MP4.")
                );
                return false;
            }
            return true;
        } @catch (NSException *exception) {
            kiri_write_error(errorBuffer, errorCapacity, exception.reason);
            return false;
        }
    }
}

void kiri_macos_encoder_cancel(void *rawEncoder) {
    @autoreleasepool {
        KiriMacEncoder *encoder = (__bridge KiriMacEncoder *)rawEncoder;
        [encoder.writer cancelWriting];
        NSString *path = encoder.writer.outputURL.path;
        if (path.length > 0) {
            [[NSFileManager defaultManager] removeItemAtPath:path error:nil];
        }
    }
}

void kiri_macos_encoder_release(void *rawEncoder) {
    if (rawEncoder != NULL) {
        CFBridgingRelease(rawEncoder);
    }
}

bool kiri_macos_probe_media(
    const char *path,
    int64_t *width,
    int64_t *height,
    double *duration,
    char *errorBuffer,
    size_t errorCapacity
) {
    @autoreleasepool {
        @try {
            NSString *inputPath = kiri_path(path);
            AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:inputPath]
                                                    options:nil];
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
            AVAssetTrack *track = [[asset tracksWithMediaType:AVMediaTypeVideo] firstObject];
            CMTime assetDuration = asset.duration;
            CGSize naturalSize = track.naturalSize;
            CGAffineTransform transform = track.preferredTransform;
#pragma clang diagnostic pop
            if (track == nil || !CMTIME_IS_NUMERIC(assetDuration)) {
                kiri_write_error(errorBuffer, errorCapacity, @"The video track could not be read.");
                return false;
            }
            CGSize transformed = CGSizeApplyAffineTransform(naturalSize, transform);
            int64_t videoWidth = (int64_t)llround(fabs(transformed.width));
            int64_t videoHeight = (int64_t)llround(fabs(transformed.height));
            double seconds = CMTimeGetSeconds(assetDuration);
            if (videoWidth <= 0 || videoHeight <= 0 || !isfinite(seconds) || seconds <= 0) {
                kiri_write_error(errorBuffer, errorCapacity, @"The video metadata is invalid.");
                return false;
            }
            *width = videoWidth;
            *height = videoHeight;
            *duration = seconds;
            return true;
        } @catch (NSException *exception) {
            kiri_write_error(errorBuffer, errorCapacity, exception.reason);
            return false;
        }
    }
}

bool kiri_macos_has_audio_track(
    const char *path,
    bool *hasAudio,
    char *errorBuffer,
    size_t errorCapacity
) {
    @autoreleasepool {
        @try {
            NSString *inputPath = kiri_path(path);
            if (inputPath.length == 0 || hasAudio == NULL) {
                kiri_write_error(errorBuffer, errorCapacity, @"The media path is invalid.");
                return false;
            }
            AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:inputPath]
                                                    options:nil];
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
            NSArray<AVAssetTrack *> *videoTracks = [asset tracksWithMediaType:AVMediaTypeVideo];
            NSArray<AVAssetTrack *> *audioTracks = [asset tracksWithMediaType:AVMediaTypeAudio];
#pragma clang diagnostic pop
            if (videoTracks.count == 0) {
                kiri_write_error(errorBuffer, errorCapacity, @"The video track could not be read.");
                return false;
            }
            *hasAudio = audioTracks.count > 0;
            return true;
        } @catch (NSException *exception) {
            kiri_write_error(errorBuffer, errorCapacity, exception.reason);
            return false;
        }
    }
}

bool kiri_macos_merge_segments(
    const char *const *paths,
    size_t pathCount,
    const char *outputPath,
    char *errorBuffer,
    size_t errorCapacity
) {
    @autoreleasepool {
        @try {
            if (paths == NULL || pathCount < 2 || kiri_path(outputPath).length == 0) {
                kiri_write_error(errorBuffer, errorCapacity, @"The native segment list is invalid.");
                return false;
            }
            AVMutableComposition *composition = [AVMutableComposition composition];
            NSMutableArray<AVMutableCompositionTrack *> *videoDestinations = [NSMutableArray array];
            NSMutableArray<AVMutableCompositionTrack *> *audioDestinations = [NSMutableArray array];
            CMTime insertionTime = kCMTimeZero;

            for (size_t pathIndex = 0; pathIndex < pathCount; pathIndex += 1) {
                NSString *segmentPath = kiri_path(paths[pathIndex]);
                AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:segmentPath]
                                                        options:nil];
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
                CMTime segmentDuration = asset.duration;
                NSArray<AVAssetTrack *> *videoTracks = [asset tracksWithMediaType:AVMediaTypeVideo];
                NSArray<AVAssetTrack *> *audioTracks = [asset tracksWithMediaType:AVMediaTypeAudio];
#pragma clang diagnostic pop
                if (!CMTIME_IS_NUMERIC(segmentDuration) || videoTracks.count == 0) {
                    kiri_write_error(errorBuffer, errorCapacity, @"A recording segment is invalid.");
                    return false;
                }
                NSArray<NSArray<AVAssetTrack *> *> *sourceGroups = @[videoTracks, audioTracks];
                NSArray<NSMutableArray<AVMutableCompositionTrack *> *> *destinationGroups =
                    @[videoDestinations, audioDestinations];
                NSArray<AVMediaType> *mediaTypes = @[AVMediaTypeVideo, AVMediaTypeAudio];
                CMTimeRange range = CMTimeRangeMake(kCMTimeZero, segmentDuration);
                for (NSUInteger groupIndex = 0; groupIndex < sourceGroups.count; groupIndex += 1) {
                    NSArray<AVAssetTrack *> *sourceTracks = sourceGroups[groupIndex];
                    NSMutableArray<AVMutableCompositionTrack *> *destinations =
                        destinationGroups[groupIndex];
                    while (destinations.count < sourceTracks.count) {
                        AVMutableCompositionTrack *destination = [composition
                            addMutableTrackWithMediaType:mediaTypes[groupIndex]
                            preferredTrackID:kCMPersistentTrackID_Invalid];
                        if (destination == nil) {
                            kiri_write_error(errorBuffer, errorCapacity, @"Could not create a composition track.");
                            return false;
                        }
                        [destinations addObject:destination];
                    }
                    for (NSUInteger trackIndex = 0; trackIndex < sourceTracks.count; trackIndex += 1) {
                        AVAssetTrack *source = sourceTracks[trackIndex];
                        AVMutableCompositionTrack *destination = destinations[trackIndex];
                        NSError *insertError = nil;
                        if (![destination insertTimeRange:range
                                                 ofTrack:source
                                                  atTime:insertionTime
                                                   error:&insertError]) {
                            kiri_write_error(errorBuffer, errorCapacity, insertError.localizedDescription);
                            return false;
                        }
                        if (pathIndex == 0 && groupIndex == 0) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
                            destination.preferredTransform = source.preferredTransform;
#pragma clang diagnostic pop
                        }
                    }
                }
                insertionTime = CMTimeAdd(insertionTime, segmentDuration);
            }

            NSURL *destinationURL = [NSURL fileURLWithPath:kiri_path(outputPath)];
            [[NSFileManager defaultManager] removeItemAtURL:destinationURL error:nil];
            AVAssetExportSession *exporter = [[AVAssetExportSession alloc]
                initWithAsset:composition
                presetName:AVAssetExportPresetPassthrough];
            if (exporter == nil) {
                kiri_write_error(errorBuffer, errorCapacity, @"Could not create the native MP4 exporter.");
                return false;
            }
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
            exporter.outputURL = destinationURL;
            exporter.outputFileType = AVFileTypeMPEG4;
            exporter.shouldOptimizeForNetworkUse = YES;
            dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
            [exporter exportAsynchronouslyWithCompletionHandler:^{
                dispatch_semaphore_signal(semaphore);
            }];
            long waitStatus = dispatch_semaphore_wait(
                semaphore,
                dispatch_time(DISPATCH_TIME_NOW, (int64_t)(600 * NSEC_PER_SEC))
            );
            if (waitStatus != 0) {
                [exporter cancelExport];
                [[NSFileManager defaultManager] removeItemAtURL:destinationURL error:nil];
                kiri_write_error(errorBuffer, errorCapacity, @"The native MP4 merge timed out.");
                return false;
            }
            if (exporter.status != AVAssetExportSessionStatusCompleted) {
                NSString *message = exporter.error.localizedDescription ?: @"The native MP4 merge failed.";
                [[NSFileManager defaultManager] removeItemAtURL:destinationURL error:nil];
                kiri_write_error(errorBuffer, errorCapacity, message);
                return false;
            }
#pragma clang diagnostic pop
            return true;
        } @catch (NSException *exception) {
            kiri_write_error(errorBuffer, errorCapacity, exception.reason);
            return false;
        }
    }
}

bool kiri_macos_export_gif(
    const char *sourcePath,
    const char *outputPath,
    uint32_t maxLongEdge,
    uint32_t fps,
    int64_t *width,
    int64_t *height,
    double *duration,
    char *errorBuffer,
    size_t errorCapacity
) {
    @autoreleasepool {
        @try {
            if (maxLongEdge == 0 || fps == 0) {
                kiri_write_error(errorBuffer, errorCapacity, @"The GIF configuration is invalid.");
                return false;
            }
            NSURL *sourceURL = [NSURL fileURLWithPath:kiri_path(sourcePath)];
            NSURL *destinationURL = [NSURL fileURLWithPath:kiri_path(outputPath)];
            AVURLAsset *asset = [AVURLAsset URLAssetWithURL:sourceURL options:nil];
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
            AVAssetTrack *track = [[asset tracksWithMediaType:AVMediaTypeVideo] firstObject];
            CMTime assetDuration = asset.duration;
            CGSize naturalSize = track.naturalSize;
            CGAffineTransform transform = track.preferredTransform;
#pragma clang diagnostic pop
            double seconds = CMTimeGetSeconds(assetDuration);
            if (track == nil || !isfinite(seconds) || seconds <= 0) {
                kiri_write_error(errorBuffer, errorCapacity, @"The GIF source video is invalid.");
                return false;
            }
            CGSize transformed = CGSizeApplyAffineTransform(naturalSize, transform);
            CGFloat sourceWidth = fabs(transformed.width);
            CGFloat sourceHeight = fabs(transformed.height);
            if (sourceWidth <= 0 || sourceHeight <= 0) {
                kiri_write_error(errorBuffer, errorCapacity, @"The GIF source dimensions are invalid.");
                return false;
            }
            CGFloat scale = MIN(1.0, (CGFloat)maxLongEdge / MAX(sourceWidth, sourceHeight));
            CGSize targetSize = CGSizeMake(
                MAX(1, round(sourceWidth * scale)),
                MAX(1, round(sourceHeight * scale))
            );
            double rawFrameCount = ceil(seconds * (double)fps);
            if (!isfinite(rawFrameCount) || rawFrameCount < 1 || rawFrameCount > (double)NSUIntegerMax) {
                kiri_write_error(errorBuffer, errorCapacity, @"The GIF would contain too many frames.");
                return false;
            }
            NSUInteger frameCount = (NSUInteger)rawFrameCount;
            [[NSFileManager defaultManager] removeItemAtURL:destinationURL error:nil];
            CGImageDestinationRef destination = CGImageDestinationCreateWithURL(
                (__bridge CFURLRef)destinationURL,
                (__bridge CFStringRef)UTTypeGIF.identifier,
                frameCount,
                NULL
            );
            if (destination == NULL) {
                kiri_write_error(errorBuffer, errorCapacity, @"Could not create the GIF destination.");
                return false;
            }
            NSDictionary *gifProperties = @{
                (NSString *)kCGImagePropertyGIFDictionary: @{
                    (NSString *)kCGImagePropertyGIFLoopCount: @0,
                },
            };
            CGImageDestinationSetProperties(destination, (__bridge CFDictionaryRef)gifProperties);

            AVAssetImageGenerator *generator = [AVAssetImageGenerator assetImageGeneratorWithAsset:asset];
            generator.appliesPreferredTrackTransform = YES;
            generator.maximumSize = targetSize;
            generator.requestedTimeToleranceBefore = kCMTimeZero;
            generator.requestedTimeToleranceAfter = CMTimeMake(1, fps);
            NSDictionary *frameProperties = @{
                (NSString *)kCGImagePropertyGIFDictionary: @{
                    (NSString *)kCGImagePropertyGIFDelayTime: @(1.0 / (double)fps),
                },
            };
            size_t outputWidth = 0;
            size_t outputHeight = 0;
            for (NSUInteger frameIndex = 0; frameIndex < frameCount; frameIndex += 1) {
                @autoreleasepool {
                    double requestedSeconds = MIN(seconds - 0.001, (double)frameIndex / (double)fps);
                    CMTime requestedTime = CMTimeMakeWithSeconds(MAX(0, requestedSeconds), 600);
                    NSError *frameError = nil;
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
                    CGImageRef image = [generator copyCGImageAtTime:requestedTime
                                                          actualTime:NULL
                                                               error:&frameError];
#pragma clang diagnostic pop
                    if (image == NULL) {
                        CFRelease(destination);
                        [[NSFileManager defaultManager] removeItemAtURL:destinationURL error:nil];
                        kiri_write_error(errorBuffer, errorCapacity, frameError.localizedDescription);
                        return false;
                    }
                    if (outputWidth == 0) {
                        outputWidth = CGImageGetWidth(image);
                        outputHeight = CGImageGetHeight(image);
                    }
                    CGImageDestinationAddImage(
                        destination,
                        image,
                        (__bridge CFDictionaryRef)frameProperties
                    );
                    CGImageRelease(image);
                }
            }
            BOOL finalized = CGImageDestinationFinalize(destination);
            CFRelease(destination);
            if (!finalized || outputWidth == 0 || outputHeight == 0) {
                [[NSFileManager defaultManager] removeItemAtURL:destinationURL error:nil];
                kiri_write_error(errorBuffer, errorCapacity, @"The GIF could not be finalized.");
                return false;
            }
            *width = (int64_t)outputWidth;
            *height = (int64_t)outputHeight;
            *duration = seconds;
            return true;
        } @catch (NSException *exception) {
            kiri_write_error(errorBuffer, errorCapacity, exception.reason);
            return false;
        }
    }
}

bool kiri_macos_video_first_frame_png(
    const char *sourcePath,
    const char *outputPath,
    uint32_t maxLongEdge,
    char *errorBuffer,
    size_t errorCapacity
) {
    @autoreleasepool {
        @try {
            if (maxLongEdge == 0) {
                kiri_write_error(errorBuffer, errorCapacity, @"The thumbnail size is invalid.");
                return false;
            }
            NSURL *sourceURL = [NSURL fileURLWithPath:kiri_path(sourcePath)];
            NSURL *destinationURL = [NSURL fileURLWithPath:kiri_path(outputPath)];
            AVURLAsset *asset = [AVURLAsset URLAssetWithURL:sourceURL options:nil];
            AVAssetImageGenerator *generator = [AVAssetImageGenerator assetImageGeneratorWithAsset:asset];
            generator.appliesPreferredTrackTransform = YES;
            generator.maximumSize = CGSizeMake(maxLongEdge, maxLongEdge);
            generator.requestedTimeToleranceBefore = kCMTimeZero;
            generator.requestedTimeToleranceAfter = CMTimeMake(1, 30);
            NSError *frameError = nil;
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
            CGImageRef image = [generator copyCGImageAtTime:kCMTimeZero
                                                actualTime:NULL
                                                     error:&frameError];
#pragma clang diagnostic pop
            if (image == NULL) {
                kiri_write_error(errorBuffer, errorCapacity, frameError.localizedDescription);
                return false;
            }
            [[NSFileManager defaultManager] removeItemAtURL:destinationURL error:nil];
            CGImageDestinationRef destination = CGImageDestinationCreateWithURL(
                (__bridge CFURLRef)destinationURL,
                (__bridge CFStringRef)UTTypePNG.identifier,
                1,
                NULL
            );
            if (destination == NULL) {
                CGImageRelease(image);
                kiri_write_error(errorBuffer, errorCapacity, @"Could not create the PNG thumbnail destination.");
                return false;
            }
            CGImageDestinationAddImage(destination, image, NULL);
            CGImageRelease(image);
            BOOL finalized = CGImageDestinationFinalize(destination);
            CFRelease(destination);
            if (!finalized) {
                [[NSFileManager defaultManager] removeItemAtURL:destinationURL error:nil];
                kiri_write_error(errorBuffer, errorCapacity, @"The PNG thumbnail could not be finalized.");
                return false;
            }
            return true;
        } @catch (NSException *exception) {
            kiri_write_error(errorBuffer, errorCapacity, exception.reason);
            return false;
        }
    }
}
