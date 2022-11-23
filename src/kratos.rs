use anyhow::{bail, Result};
use log::{debug, info};
use ory_kratos_client::apis::{configuration::Configuration, v0alpha2_api::to_session};
use rocket::http::Cookie;
use serde::Deserialize;

pub use ory_kratos_client::models::Identity;

///structure containing kratos config. thi to be used with figment
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Kratos {
    pub addr: String,

    #[serde(skip)]
    pub client: Option<Configuration>,
}

impl Kratos {
    ///this fuction update the kratos client
    pub fn update(&mut self) -> &mut Self {
        let kratos = &self;
        let mut client = Configuration::new();
        client.base_path = kratos.addr.clone();
        self.client = Some(client);
        self
    }
    ///validate a katos session cookie.
    ///return the user identity.
    ///return an error if its invalid or the cookie is not present.
    pub async fn validate_session(&self, cookie: &Cookie<'_>) -> Result<Identity> {
        let kratos_client = match self.client {
            Some(ref client) => client,
            None => {
                bail!("kratos is not initialized!");
            }
        };
        info!("validating session cookie");
        debug!("session cookie: {cookie}");
        let session = to_session(kratos_client, None, Some(&cookie.to_string())).await?;
        info!("session cookie successfully validated");
        Ok(*session.identity)
    }
}

#[cfg(test)]
mod kratos_test {
    use httpmock::prelude::*;
    use rocket::http::Cookie;

    use super::*;

    #[tokio::test]
    async fn test_validate_session() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/sessions/whoami");
                then.status(200)
                    .body(r#"{"id": "1","identity": {"id":"1","schema_id":"1","schema_url":"test.com" }}"#);
            })
            .await;
        let mut kratos = Kratos {
            client: Some(Configuration::new()),
            ..Default::default()
        };
        let mut client = kratos.client.unwrap();
        client.base_path = server.base_url();
        kratos.client = Some(client);
        let cookies = Cookie::new("ory_kratos_session", "test");
        let res = kratos.validate_session(&cookies).await;
        mock.assert_async().await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_validate_session_error() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/sessions/whoami");
                then.status(500);
            })
            .await;
        let mut kratos = Kratos {
            client: Some(Configuration::new()),
            ..Default::default()
        };
        let mut client = kratos.client.unwrap();
        client.base_path = server.base_url();
        kratos.client = Some(client);
        let cookies = Cookie::new("ory_kratos_session", "test");
        let res = kratos.validate_session(&cookies).await;
        mock.assert_async().await;
        assert!(res.is_err());
    }
}
