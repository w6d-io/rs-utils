use anyhow::{anyhow, Result};
use log::debug;
// use reqwest::StatusCode;
use crate::config;
use s3::{
    creds::Credentials, error::S3Error, region::Region, request_trait::ResponseData,
    serde_types::ListBucketResult, Bucket,
};
use serde::Deserialize;
use time::OffsetDateTime;

#[derive(Deserialize)]
pub struct Minio {
    pub name: String,
    pub service: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub security_token: Option<String>,
    pub session_token: Option<String>,
    pub expiration: Option<OffsetDateTime>,
    #[serde(skip_deserializing)]
    pub client: Option<Client>,
}

impl Minio {
    pub fn update(mut self) -> Result<Self> {
        self.client = Some(Client::new(&self)?);
        Ok(self)
    }
}

impl Default for Minio {
    fn default() -> Self {
        Minio {
            name: "".to_owned(),
            service: "".to_owned(),
            access_key: None,
            secret_key: None,
            security_token: None,
            session_token: None,
            expiration: None,
            client: None,
        }
    }
}

// use super::config;

/* fn http_code_handler(code: u16) -> Result<StatusCode> {
    let status = StatusCode::from_u16(code)?;
    let reason = status.canonical_reason().unwrap_or("unknow");
    info!("code {}: {}", code, reason);
    Ok(status)
} */

pub struct Client(Bucket);

impl Client {
    ///create a new minio client with the given config
    pub fn new(config: &config::Minio) -> Result<Self, S3Error> {
        let credentials = Credentials {
            access_key: config.access_key.to_owned(),
            secret_key: config.secret_key.to_owned(),
            security_token: config.security_token.to_owned(),
            session_token: config.session_token.to_owned(),
            expiration: config.expiration,
        };
        let region = Region::Custom {
            region: "us-east-1".into(),
            endpoint: config.service.to_owned(),
        };
        let bucket = Bucket::new(&config.name, region, credentials)?;
        Ok(Client(bucket.with_path_style()))
    }

    pub async fn put_object<S>(&self, data: &[u8], path: S) -> Result<ResponseData, S3Error>
    where
        S: AsRef<str>,
    {
        self.0.put_object(path, data).await
    }

    pub async fn _get_object<S>(&self, path: S) -> Result<ResponseData, S3Error>
    where
        S: AsRef<str>,
    {
        self.0.get_object(path).await
    }

    pub async fn _list_object(&self) -> Result<ListBucketResult> {
        let raw_list = self.0.list("/".to_string(), None).await?;
        debug!("raw bucket object: {:#?}", raw_list);
        let list = raw_list
            .get(0)
            .ok_or_else(|| anyhow!("the bucket is empty"))?;
        Ok(list.to_owned())
    }
}
/* #[cfg(test)]
mod tests {
    use std::str;

    use super::*;
    use rand::{distributions::Alphanumeric, thread_rng, Rng};

    fn get_random_string(len: usize) -> String {
        let rng: Vec<u8> = thread_rng().sample_iter(&Alphanumeric).take(len).collect();
        str::from_utf8(&rng).unwrap().to_string()
    }

    #[tokio::test]
    async fn test_minio() {
        let config = Minio {
            service: "https://play.min.io:9000".to_owned(),
            name: "0000".to_owned(),
            access_key: Some("Q3AM3UQ867SPQQA43P2F".to_owned()),
            secret_key: Some("zuf+tfteSlswRu7BJ86wekitnifILbZam1KYY3TG".to_owned()),
            ..Default::default()
        };
        let client = Client::new(&config).unwrap();

        let filename = get_random_string(10) + "__AAA";
        let object = b"LULULULULULULULULU! LALALALA!";
        let res = client.put_object(object, &filename).await;
        assert!(res.is_ok());
    }
} */
