use actix_multipart::form::{tempfile::TempFile, text::Text, MultipartForm};
use actix_web::{web, HttpResponse, Result};

#[derive(Debug, MultipartForm)]
pub struct UploadForm {
    #[multipart(rename = "target_type")]
    pub target_type: Text<String>,
    #[multipart(rename = "target_id_or_path")]
    pub target_id_or_path: Text<String>,
    #[multipart(rename = "operation")]
    pub operation: Text<String>,
    #[multipart(rename = "audio_file")]
    pub audio_file: Option<TempFile>,
}
