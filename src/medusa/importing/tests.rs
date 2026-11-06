use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::Importing as _;
use serde_json::json;

use crate::medusa::tests::{asked, json, medusa};

#[tokio::test]
async fn it_hardlinks_where_its_process_method_is_a_hard_link() {
    for (held, hardlinks) in [
        (json!({ "processMethod": "hardlink" }), true),
        (json!({ "processMethod": "copy" }), false),
        (
            json!({ "processMethod": "hardlink", "specificProcessMethod": true, "processMethodNzb": "hardlink", "processMethodTorrent": "copy" }),
            false,
        ),
        (
            json!({ "processMethod": "copy", "specificProcessMethod": true, "processMethodNzb": "hardlink", "processMethodTorrent": "hardlink" }),
            true,
        ),
    ] {
        let fake = Fake::always(json(200, &held));
        assert_eq!(medusa(&fake).hardlinks().await.ok(), Some(hardlinks));
        assert_eq!(
            asked(&fake),
            [(
                Method::Get,
                "/api/v2/config/postprocessing".to_owned(),
                None
            )]
        );
    }
}

#[tokio::test]
async fn hardlinking_is_set_for_every_download_and_copying_where_it_is_not() {
    for (hardlink, method) in [(true, "hardlink"), (false, "copy")] {
        let written = json!({ "postProcessing": {
            "processMethod": method,
            "processMethodNzb": method,
            "processMethodTorrent": method,
        } });
        let fake = Fake::always(Answer::reply(200, written.to_string()));
        assert!(medusa(&fake).set_hardlinks(hardlink).await.is_ok());
        assert_eq!(
            asked(&fake),
            [(
                Method::Patch,
                "/api/v2/config/main".to_owned(),
                Some(written)
            )]
        );
    }
}
