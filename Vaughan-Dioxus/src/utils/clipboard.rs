use std::fmt;

#[derive(Debug)]
pub struct ClipboardError(pub String);

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ClipboardError {}

/// Copy text to clipboard (desktop supported; web/mobile stubbed for now).
pub fn copy_text(text: &str) -> Result<(), ClipboardError> {
    #[cfg(feature = "desktop")]
    {
        arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(text.to_string()))
            .map_err(|e| ClipboardError(e.to_string()))
    }

    #[cfg(not(feature = "desktop"))]
    {
        let _ = text;
        Err(ClipboardError(
            "clipboard not implemented on this target yet".into(),
        ))
    }
}
