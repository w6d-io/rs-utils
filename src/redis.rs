use anyhow::Result;
use redis::Cmd;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct Redis {
    pub addr: String,
    #[serde(skip_deserializing)]
    pub client: Option<Client>,
}

impl Redis {
    pub fn update(mut self) -> Result<Self> {
        let client = Client::new(&self)?;
        self.client = Some(client);
        Ok(self)
    }
}

#[derive(Clone, Debug)]
pub struct Client(redis::Client);

impl Client {
    pub fn new(config: &Redis) -> Result<Self> {
        let client = redis::Client::open(config.addr.clone())?;
        Ok(Client(client))
    }

    pub async fn hset(&self, key: &str, field: &str, value: &str) -> Result<()> {
        let mut connection = self.0.get_tokio_connection().await?;
        Cmd::hset_nx(key, field, value)
            .query_async(&mut connection)
            .await?;
        Ok(())
    }

    pub async fn hexists(&self, key: &str, field: &str) -> Result<bool> {
        let mut connection = self.0.get_tokio_connection().await?;
        let res: bool = Cmd::hexists(key, field)
            .query_async(&mut connection)
            .await?;
        Ok(res)
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
        let mut connection = self.0.get_tokio_connection().await?;
        let res: bool = Cmd::exists(key).query_async(&mut connection).await?;
        Ok(res)
    }
}