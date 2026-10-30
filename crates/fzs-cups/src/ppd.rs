//! Embedded PostScript Printer Description (PPD) for FZS thermal printer.
//!
//! Validated with official Apple `cupstestppd` tool (PASS).
//! Bypasses `rastertopwg` underflow bug by directly accepting `application/vnd.cups-raster`.

use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_PPD_CONTENT: &str = r#"*PPD-Adobe: "4.3"
*FormatVersion: "4.3"
*FileVersion: "1.1"
*LanguageVersion: English
*LanguageEncoding: ISOLatin1
*PCFileName: "FZSTHERM.PPD"
*Manufacturer: "FZS"
*Product: "(Thermal Printer)"
*ModelName: "FZS C3100A Thermal Printer"
*ShortNickName: "FZS-C3100A"
*NickName: "FZS-C3100A Thermal Printer"
*PSVersion: "(3010.000) 550"
*LanguageLevel: "3"
*ColorDevice: False
*DefaultColorSpace: Gray
*FileSystem: False
*Throughput: "1"
*LandscapeOrientation: Plus90
*TTRasterizer: Type42

*cupsVersion: 2.0
*cupsModelNumber: 1
*cupsManualCopies: True
*cupsFilter: "application/vnd.cups-raster 0 -"

*OpenUI *Resolution/Output Resolution: PickOne
*OrderDependency: 10 AnySetup *Resolution
*DefaultResolution: 203x281dpi
*Resolution 203x281dpi/203 x 281 DPI (Hardware Native): "<</HWResolution[203 281.5]>>setpagedevice"
*Resolution 203dpi/203 x 203 DPI (Square Pixel): "<</HWResolution[203 203]>>setpagedevice"
*CloseUI: *Resolution

*OpenUI *PageSize/Media Size: PickOne
*OrderDependency: 10 AnySetup *PageSize
*DefaultPageSize: A4
*PageSize A4/A4 (210 x 297 mm): "<</PageSize[595 842]/ImagingBBox null>>setpagedevice"
*PageSize A5/A5 (148 x 210 mm): "<</PageSize[420 595]/ImagingBBox null>>setpagedevice"
*PageSize Roll210/Continuous Roll: "<</PageSize[595 1417]/ImagingBBox null>>setpagedevice"
*CloseUI: *PageSize

*OpenUI *PageRegion/Media Size: PickOne
*OrderDependency: 10 AnySetup *PageRegion
*DefaultPageRegion: A4
*PageRegion A4/A4: "<</PageSize[595 842]/ImagingBBox null>>setpagedevice"
*PageRegion A5/A5: "<</PageSize[420 595]/ImagingBBox null>>setpagedevice"
*PageRegion Roll210/Continuous Roll: "<</PageSize[595 1417]/ImagingBBox null>>setpagedevice"
*CloseUI: *PageRegion

*DefaultImageableArea: A4
*ImageableArea A4/A4: "0 0 595 842"
*ImageableArea A5/A5: "0 0 420 595"
*ImageableArea Roll210/Continuous Roll: "0 0 595 1417"

*DefaultPaperDimension: A4
*PaperDimension A4/A4: "595 842"
*PaperDimension A5/A5: "420 595"
*PaperDimension Roll210/Continuous Roll: "595 1417"

*OpenUI *ColorModel/Color Mode: PickOne
*OrderDependency: 10 AnySetup *ColorModel
*DefaultColorModel: Gray
*ColorModel Gray/Grayscale (8-bit): "<</cupsColorSpace 0/cupsColorOrder 0/cupsBitsPerColor 8/cupsBitsPerPixel 8>>setpagedevice"
*ColorModel FastGray/Monochrome (1-bit): "<</cupsColorSpace 0/cupsColorOrder 0/cupsBitsPerColor 1/cupsBitsPerPixel 1>>setpagedevice"
*CloseUI: *ColorModel

*OpenGroup: General/General

*OpenUI *Dither/Dithering Algorithm: PickOne
*OrderDependency: 20 AnySetup *Dither
*DefaultDither: Auto
*Dither Auto/Smart Hybrid (Text & Graphics): "<</cupsInteger0 0>>setpagedevice"
*Dither FloydSteinberg/Floyd-Steinberg (Photos): "<</cupsInteger0 1>>setpagedevice"
*Dither Bayer/Bayer 4x4 (Pattern): "<</cupsInteger0 2>>setpagedevice"
*Dither Threshold/Sharp Threshold (Text only): "<</cupsInteger0 3>>setpagedevice"
*CloseUI: *Dither

*OpenUI *Darkness/Heating Density: PickOne
*OrderDependency: 20 AnySetup *Darkness
*DefaultDarkness: Normal
*Darkness VeryLight/Very Light (140): "<</cupsInteger1 140>>setpagedevice"
*Darkness Light/Light (160): "<</cupsInteger1 160>>setpagedevice"
*Darkness Normal/Normal (180): "<</cupsInteger1 180>>setpagedevice"
*Darkness Dark/Dark (200): "<</cupsInteger1 200>>setpagedevice"
*Darkness VeryDark/Very Dark (220): "<</cupsInteger1 220>>setpagedevice"
*CloseUI: *Darkness

*OpenUI *MediaType/Paper Type: PickOne
*OrderDependency: 20 AnySetup *MediaType
*DefaultMediaType: Continuous
*MediaType Continuous/Continuous Roll: "<</cupsInteger2 0>>setpagedevice"
*MediaType Folded/Fanfold Perforated: "<</cupsInteger2 1>>setpagedevice"
*CloseUI: *MediaType

*OpenUI *FeedSteps/Post-Print Eject Steps: PickOne
*OrderDependency: 20 AnySetup *FeedSteps
*DefaultFeedSteps: Default
*FeedSteps Default/Automatic (By Model): "<</cupsInteger3 0>>setpagedevice"
*FeedSteps Short/Short (100 steps): "<</cupsInteger3 100>>setpagedevice"
*FeedSteps Normal/Standard (162 steps): "<</cupsInteger3 162>>setpagedevice"
*FeedSteps Long/Long (250 steps): "<</cupsInteger3 250>>setpagedevice"
*CloseUI: *FeedSteps

*CloseGroup: General
"#;

/// Write embedded PPD to specified path or return standard temporary path.
pub fn ensure_ppd_file(dest: Option<&Path>) -> std::io::Result<PathBuf> {
    let path = match dest {
        Some(p) => p.to_path_buf(),
        None => {
            let dir = std::env::temp_dir().join("fzs-printer");
            fs::create_dir_all(&dir)?;
            dir.join("FZS-Thermal.ppd")
        }
    };

    fs::write(&path, DEFAULT_PPD_CONTENT)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn test_ppd_generation_and_cupstestppd() {
        let ppd_path = ensure_ppd_file(None).expect("create ppd");
        assert!(ppd_path.exists());
        let content = fs::read_to_string(&ppd_path).expect("read ppd");
        assert!(content.contains("*ModelName: \"FZS C3100A Thermal Printer\""));

        // If cupstestppd exists, test it!
        if let Ok(output) = Command::new("cupstestppd").arg(&ppd_path).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success(),
                "cupstestppd failed:\nSTDOUT:\n{}\nSTDERR:\n{}",
                stdout,
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(stdout.contains("PASS"));
        }
    }
}
