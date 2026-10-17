//! Icons. GPUI Kit's default bundle covers the icons its own components use;
//! the extra Lucide icons the pages need are embedded selectively rather than
//! registering all 1,830.

use std::borrow::Cow;

use gpui_kit::assets::{icon_assets, Assets};
use gpui_kit::{AssetSource, Result, SharedString};

icon_assets!(
    ExtraIcons,
    [
        Activity,
        Cloud,
        Command,
        Crown,
        Download,
        FingerprintPattern,
        Gauge,
        KeyRound,
        Mail,
        MessageSquare,
        Monitor,
        Package,
        Pencil,
        Power,
        RotateCcw,
        Save,
        Send,
        Share2,
        Shield,
        ShieldCheck,
        SlidersHorizontal,
        Sparkles,
        Trash,
        UserPlus,
        Users,
        Wifi,
        WifiOff,
        X,
        Zap,
    ]
);

/// The default component icons plus [`ExtraIcons`].
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match ExtraIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut all = ExtraIcons.list(path)?;
        all.extend(Assets.list(path)?);
        Ok(all)
    }
}
