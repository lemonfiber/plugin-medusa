use async_trait::async_trait;
use lemonfiber_ports::service::{Failure, Importing};
use serde::Deserialize;
use serde_json::json;

use super::Medusa;

const POST_PROCESSING: &str = "/config/postprocessing";

const HARDLINK: &str = "hardlink";

const COPY: &str = "copy";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PostProcessing {
    #[serde(default)]
    process_method: String,
    #[serde(default)]
    specific_process_method: bool,
    #[serde(default)]
    process_method_nzb: String,
    #[serde(default)]
    process_method_torrent: String,
}

#[async_trait]
impl Importing for Medusa {
    async fn hardlinks(&self) -> Result<bool, Failure> {
        let held: PostProcessing = self
            .read(
                POST_PROCESSING,
                "Medusa's post-processing settings could not be read",
            )
            .await?;
        Ok(if held.specific_process_method {
            held.process_method_nzb == HARDLINK && held.process_method_torrent == HARDLINK
        } else {
            held.process_method == HARDLINK
        })
    }

    async fn set_hardlinks(&self, hardlink: bool) -> Result<(), Failure> {
        let method = if hardlink { HARDLINK } else { COPY };
        self.configure(&json!({
            "postProcessing": {
                "processMethod": method,
                "processMethodNzb": method,
                "processMethodTorrent": method,
            }
        }))
        .await
    }
}

#[cfg(test)]
mod tests;
