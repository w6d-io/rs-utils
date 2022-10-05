use anyhow::bail;
use anyhow::Result;
use redis::Cmd;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Redis {
    pub addr: String,
    pub password: Option<String>,
    pub user: Option<String>,
    #[serde(skip_deserializing)]
    pub client: Option<Client>,
}

impl Redis {
    pub fn update(&mut self) -> Result<&mut Self> {
        let client = Client::new(self)?;
        self.client = Some(client);
        Ok(self)
    }
}

#[derive(Clone, Debug)]
pub struct Client(redis::Client);

impl Client {
    pub fn new(config: &Redis) -> Result<Self> {
        let mut url = String::from("Redis://");
        match config.password {
            Some(ref password) => {
                if let Some(ref user) = config.user {
                    url = url + user as &str + ":";
                }
                url = url + password as &str + "@";
            }
            None => {
                if config.user.is_some() {
                    bail!("Error: provided redis user without password");
                }
            }
        }
        url += &config.addr as &str;

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
