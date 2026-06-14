use std::path::PathBuf;

use imessage_database::tables::attachment::MediaType;

use super::target::AttachmentTarget;

// MARK: Plan
/// The work an [`AttachmentManager`](super::AttachmentManager) resolved for an
/// attachment, ready to be executed.
pub(crate) enum Plan {
    /// Copying is disabled; nothing to do.
    Skip,
    /// Existing output at `path` should be reused. When it was a conversion,
    /// `media_type` carries the converted type to record.
    Reuse {
        path: PathBuf,
        media_type: Option<MediaType<'static>>,
    },
    /// No existing copy; the attachment must be copied or converted to `target`.
    Process { target: AttachmentTarget },
}
