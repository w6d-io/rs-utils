use anyhow::{bail, Result};
use log::{debug, info};
use ory_kratos_client::apis::{configuration::Configuration, v0alpha2_api::to_session};
use rocket::http::Cookie;

///validate a katos session cookie.
///return an error if its invalid or the cookie is not present.
pub async fn validate_session(kratos: &Option<Configuration>, session: &Cookie<'_>) -> Result<()> {
    let kratos_client = match kratos {
        Some(client) => client,
        None => {
            bail!("kratos is not initialized!");
        }
    };
    info!("validating session cookie");
    debug!("session cookie: {session}");
    to_session(kratos_client, None, Some(&session.to_string())).await?;
    info!("session cookie successfully validated");
    Ok(())
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
        let mut kratos = Configuration::new();
        kratos.base_path = server.base_url();
        let cookies = Cookie::new("ory_kratos_session", "test");
        let res = validate_session(&Some(kratos), &cookies).await;
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
        let mut kratos = Configuration::new();
        kratos.base_path = server.base_url();
        let cookies = Cookie::new("ory_kratos_session", "test");
        let res = validate_session(&Some(kratos), &cookies).await;
        mock.assert_async().await;
        assert!(res.is_err());
    }
}
