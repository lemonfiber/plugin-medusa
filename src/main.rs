//! The adapter's process: read the plugin's key, then answer the core until stopped.

use std::process::ExitCode;
use std::sync::Arc;

use lemonfiber_adapter_kit::credential::Upstream;
use lemonfiber_adapter_kit::settings::{asked, Asked, Settings};
use lemonfiber_adapter_kit::web::Web;
use lemonfiber_contract::adapter::{about_path, UPSTREAM_PASSWORD, UPSTREAM_USERNAME};
use medusa_adapter::medusa::Medusa;
use medusa_adapter::thetvdb::TheTvdb;

#[tokio::main]
async fn main() -> ExitCode {
    let outcome = match asked(std::env::args().skip(1), medusa_adapter::adapter::ADDRESS) {
        Ok(Asked::Serve(settings)) => serve(&settings).await,
        Ok(Asked::Health(settings)) => health(&settings).await,
        Err(why) => Err(why),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("medusa-adapter: {why}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(settings: &Settings) -> Result<(), String> {
    let key = lemonfiber_adapter_kit::key_in(&settings.configuration)
        .map_err(|why| format!("the plugin's key could not be read: {why}"))?;
    if key.is_empty() {
        return Err("the plugin's key is empty".to_owned());
    }
    let patience = medusa_adapter::adapter::patience(
        std::env::var(medusa_adapter::adapter::IMPORT_STUCK_AFTER)
            .ok()
            .as_deref(),
    )?;
    let env = |name: &str| std::env::var(name).ok();
    let sign_in = medusa_adapter::adapter::thetvdb(
        env(medusa_adapter::adapter::THETVDB_LOOKUP).as_deref(),
        env(medusa_adapter::adapter::THETVDB_KEY).as_deref(),
        env(medusa_adapter::adapter::THETVDB_PIN).as_deref(),
    )?;
    let web = Arc::new(Web::new().map_err(|why| format!("no HTTP client could be made: {why}"))?);
    let lookup = sign_in.map(|sign_in| {
        Arc::new(TheTvdb::new(
            Arc::clone(&web) as Arc<dyn lemonfiber_ports::http::Http>,
            medusa_adapter::thetvdb::ADDRESS,
            sign_in,
        ))
    });
    let medusa = Medusa::new(
        web,
        settings.upstream.clone(),
        Arc::new(Upstream::in_directory(
            &settings.configuration,
            medusa_adapter::adapter::API_KEY,
        )),
        Arc::new(Upstream::in_directory(
            &settings.configuration,
            UPSTREAM_USERNAME,
        )),
        Arc::new(Upstream::in_directory(
            &settings.configuration,
            UPSTREAM_PASSWORD,
        )),
    )
    .waiting_on_imports(patience)
    .looking_up(lookup);
    let listener = tokio::net::TcpListener::bind(&settings.listen)
        .await
        .map_err(|why| format!("{} could not be listened on: {why}", settings.listen))?;
    eprintln!("medusa-adapter: answering on {}", settings.listen);
    lemonfiber_adapter_kit::serving(
        medusa_adapter::adapter::recounting_kit(
            key,
            Arc::new(medusa),
            settings.configuration.clone(),
            Arc::new(Logged),
        ),
        listener,
        stopped(),
    )
    .await
    .map_err(|why| format!("serving stopped: {why}"))
}

async fn health(settings: &Settings) -> Result<(), String> {
    let key = lemonfiber_adapter_kit::key_in(&settings.configuration)
        .map_err(|why| format!("the plugin's key could not be read: {why}"))?;
    let port = settings
        .listen
        .rsplit_once(':')
        .map(|(_, port)| port)
        .ok_or_else(|| format!("{} names no port", settings.listen))?;
    let answer = lemonfiber_adapter_kit::web::client_builder()
        .build()
        .map_err(|why| format!("no client could be built: {why}"))?
        .get(format!("http://127.0.0.1:{port}{}", about_path()))
        .bearer_auth(key)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .map_err(|why| format!("nothing answered: {why}"))?;
    if answer.status().is_success() {
        Ok(())
    } else {
        Err(format!("it answered {}", answer.status()))
    }
}

struct Logged;

#[async_trait::async_trait]
impl lemonfiber_ports::narration::Narrator for Logged {
    async fn say(&self, said: &str) {
        eprintln!("medusa-adapter: {said}");
    }
}

async fn stopped() {
    let interrupted = tokio::signal::ctrl_c();
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut terminated) => {
            tokio::select! {
                _ = interrupted => {}
                _ = terminated.recv() => {}
            }
        }
        Err(_) => {
            let _ = interrupted.await;
        }
    }
}
