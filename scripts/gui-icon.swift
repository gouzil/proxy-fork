import AppKit
import Foundation

let size = NSSize(width: 512, height: 512)
guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 512, pixelsHigh: 512,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
    let context = NSGraphicsContext(bitmapImageRep: bitmap) else { fatalError("Icon allocation failed") }
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = context
NSColor(calibratedRed: 0.12, green: 0.25, blue: 0.42, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(origin: .zero, size: size), xRadius: 96, yRadius: 96).fill()
NSColor.white.setFill()
NSBezierPath(roundedRect: NSRect(x: 130, y: 100, width: 65, height: 310), xRadius: 16, yRadius: 16).fill()
NSBezierPath(roundedRect: NSRect(x: 170, y: 240, width: 215, height: 170), xRadius: 65, yRadius: 65).fill()
NSColor(calibratedRed: 0.12, green: 0.25, blue: 0.42, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 195, y: 290, width: 125, height: 65), xRadius: 26, yRadius: 26).fill()
context.flushGraphics()
NSGraphicsContext.restoreGraphicsState()
guard CommandLine.arguments.count == 2,
      let png = bitmap.representation(using: .png, properties: [:]) else { fatalError("Icon generation failed") }
try png.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
