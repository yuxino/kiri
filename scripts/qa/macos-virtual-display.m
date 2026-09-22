// Isolated native QA fixture; never linked into Kiri.
// CGVirtualDisplay declarations follow Chromium's system-level display tests:
// https://chromium.googlesource.com/chromium/src/+/HEAD/ui/display/mac/test/virtual_display_util_mac.mm
// clang -fobjc-arc -framework Cocoa -framework CoreGraphics \
//   scripts/qa/macos-virtual-display.m -o /tmp/kiri-virtual-display
// /tmp/kiri-virtual-display <x> <y> <scale:1|2>
// The temporary display disappears when this process exits (or after 10 min).
#import <Cocoa/Cocoa.h>
#import <CoreGraphics/CoreGraphics.h>

@interface CGVirtualDisplayDescriptor : NSObject
@property unsigned int vendorID, productID, serialNum, serialNumber;
@property unsigned int maxPixelsWide, maxPixelsHigh;
@property CGSize sizeInMillimeters;
@property CGPoint redPrimary, greenPrimary, bluePrimary, whitePoint;
@property(strong) NSString *name;
@property(strong) id queue;
@end
@interface CGVirtualDisplayMode : NSObject
- (id)initWithWidth:(unsigned int)width height:(unsigned int)height refreshRate:(double)rate;
@end
@interface CGVirtualDisplaySettings : NSObject
@property unsigned int hiDPI;
@property(strong) NSArray *modes;
@end
@interface CGVirtualDisplay : NSObject
@property(readonly) unsigned int displayID;
- (id)initWithDescriptor:(CGVirtualDisplayDescriptor *)descriptor;
- (BOOL)applySettings:(CGVirtualDisplaySettings *)settings;
@end

int main(int argc, const char **argv) {
    @autoreleasepool {
        if (argc != 4 || (atoi(argv[3]) != 1 && atoi(argv[3]) != 2)) {
            fprintf(stderr, "usage: kiri-virtual-display x y scale(1|2)\n");
            return 1;
        }
        [NSApplication sharedApplication];
        int scale = atoi(argv[3]);
        CGVirtualDisplayDescriptor *descriptor = [CGVirtualDisplayDescriptor new];
        descriptor.queue = dispatch_get_global_queue(QOS_CLASS_USER_INTERACTIVE, 0);
        descriptor.name = @"Kiri isolated display QA";
        descriptor.vendorID = 505;
        descriptor.productID = 21;
        descriptor.serialNum = descriptor.serialNumber = (unsigned int)getpid();
        descriptor.maxPixelsWide = 1280 * scale;
        descriptor.maxPixelsHigh = 800 * scale;
        descriptor.sizeInMillimeters = CGSizeMake(300, 190);
        descriptor.whitePoint = CGPointMake(.3125, .3291);
        descriptor.redPrimary = CGPointMake(.6797, .3203);
        descriptor.greenPrimary = CGPointMake(.2559, .6983);
        descriptor.bluePrimary = CGPointMake(.1494, .0557);
        CGVirtualDisplay *display = [[CGVirtualDisplay alloc] initWithDescriptor:descriptor];
        if (!display) return 2;
        CGVirtualDisplaySettings *settings = [CGVirtualDisplaySettings new];
        settings.hiDPI = scale == 2;
        settings.modes = @[[[CGVirtualDisplayMode alloc] initWithWidth:1280 height:800 refreshRate:60]];
        if (![display applySettings:settings]) return 3;
        [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:2]];

        CGDisplayConfigRef config;
        if (CGBeginDisplayConfiguration(&config) != kCGErrorSuccess) return 4;
        CGError position = CGConfigureDisplayOrigin(config, display.displayID, atoi(argv[1]), atoi(argv[2]));
        if (position != kCGErrorSuccess) {
            CGCancelDisplayConfiguration(config);
            return 5;
        }
        if (CGCompleteDisplayConfiguration(config, kCGConfigureForSession) != kCGErrorSuccess) return 6;
        // The OS may initially reuse a saved low-resolution mode. Select and
        // verify the backing size instead of assuming the hiDPI hint won.
        CFArrayRef modes = CGDisplayCopyAllDisplayModes(display.displayID,
            (__bridge CFDictionaryRef)@{(__bridge NSString *)kCGDisplayShowDuplicateLowResolutionModes: @YES});
        for (id value in (__bridge NSArray *)modes) {
            CGDisplayModeRef mode = (__bridge CGDisplayModeRef)value;
            if (CGDisplayModeGetWidth(mode) == 1280 && CGDisplayModeGetPixelWidth(mode) == 1280 * scale) {
                CGDisplaySetDisplayMode(display.displayID, mode, NULL);
                break;
            }
        }
        if (modes) CFRelease(modes);
        [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:2]];
        NSScreen *target = nil;
        for (NSScreen *screen in NSScreen.screens) {
            if ([screen.deviceDescription[@"NSScreenNumber"] unsignedIntValue] == display.displayID) target = screen;
        }
        if (!target || target.backingScaleFactor != scale) return 7;
        NSWindow *fixture = [[NSWindow alloc] initWithContentRect:target.frame
            styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
        fixture.backgroundColor = NSColor.whiteColor;
        fixture.title = @"Kiri display QA fixture";
        NSTextField *text = [NSTextField wrappingLabelWithString:
            @"KIRI SECOND DISPLAY\nNative capture acceptance\n1234567890"];
        text.font = [NSFont systemFontOfSize:40];
        text.textColor = NSColor.blackColor;
        text.frame = NSMakeRect(140, 260, 1000, 300);
        [fixture.contentView addSubview:text];
        [fixture orderFrontRegardless];
        NSLog(@"QA display id=%u bounds=%@ backingScale=%.0f", display.displayID,
            NSStringFromRect(NSRectFromCGRect(CGDisplayBounds(display.displayID))), target.backingScaleFactor);
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 600 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{
            [NSApp terminate:nil];
        });
        [NSApp run];
        // Keep the virtual hardware and source window alive throughout QA.
        (void)display;
        (void)fixture;
    }
    return 0;
}
