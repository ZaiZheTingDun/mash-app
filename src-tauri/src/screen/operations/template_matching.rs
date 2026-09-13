//! Arbitrary template-file matching operations.

use super::*;

impl SidecarClient {
    /// Search for an arbitrary grayscale template file within a region.
    #[allow(dead_code)]
    pub fn find_region(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let mut req = request(
            SidecarCommand::FindRegion,
            serde_json::json!({
                "templatePath": template_path.to_string_lossy(),
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "threshold": threshold,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Search for an arbitrary template across a list of scales and return
    /// the best match. The sidecar keeps the highest score even when it is
    /// below `threshold`, which lets callers include useful diagnostics.
    pub fn find_region_multiscale(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        threshold: f64,
        scales: &[f64],
    ) -> Result<ElementMatch, String> {
        let mut req = request(
            SidecarCommand::FindRegion,
            serde_json::json!({
                "templatePath": template_path.to_string_lossy(),
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "threshold": threshold,
                "scales": scales,
                "alphaMask": true,
                "alphaBackground": 224,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(format!(
                "find_region_multiscale({}): {err}",
                template_path.display()
            ));
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|region| {
            Some(NormRect {
                x: region.get("x")?.as_f64()?,
                y: region.get("y")?.as_f64()?,
                w: region.get("w")?.as_f64()?,
                h: region.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }

    /// Search for an arbitrary template file after cropping the template.
    #[allow(dead_code)]
    pub fn find_region_with_template_crop(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let mut req = request(
            SidecarCommand::FindRegion,
            serde_json::json!({
                "templatePath": template_path.to_string_lossy(),
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "templateCrop": {
                    "x": template_crop.x,
                    "y": template_crop.y,
                    "w": template_crop.w,
                    "h": template_crop.h,
                },
                "threshold": threshold,
            }),
        )?;
        if let Some((width, height)) = template_size {
            if let Some(object) = req.as_object_mut() {
                object.insert(
                    "templateSize".into(),
                    serde_json::json!({ "w": width, "h": height }),
                );
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Search for an arbitrary template file after cropping the template,
    /// returning the raw match payload even when it misses the threshold.
    #[allow(dead_code)]
    pub fn find_region_with_template_crop_full(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
    ) -> Result<ElementMatch, String> {
        let mut req = request(
            SidecarCommand::FindRegion,
            serde_json::json!({
                "templatePath": template_path.to_string_lossy(),
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "templateCrop": {
                    "x": template_crop.x,
                    "y": template_crop.y,
                    "w": template_crop.w,
                    "h": template_crop.h,
                },
                "threshold": threshold,
            }),
        )?;
        if let Some((width, height)) = template_size {
            if let Some(object) = req.as_object_mut() {
                object.insert(
                    "templateSize".into(),
                    serde_json::json!({ "w": width, "h": height }),
                );
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()));
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|region| {
            Some(NormRect {
                x: region.get("x")?.as_f64()?,
                y: region.get("y")?.as_f64()?,
                w: region.get("w")?.as_f64()?,
                h: region.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }
}
