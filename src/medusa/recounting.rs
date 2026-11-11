use async_trait::async_trait;
use lemonfiber_ports::service::{Failure, Recounting};

use super::{unsupported, Medusa};

const NOT_YET: &str = "this adapter does not set up a Medusa notifier for lemonfiber yet";

#[async_trait]
impl Recounting for Medusa {
    async fn events(
        &self,
        _address: Option<&str>,
        _pin: Option<&str>,
        _key: Option<&str>,
    ) -> Result<(), Failure> {
        Err(unsupported(NOT_YET))
    }
}
