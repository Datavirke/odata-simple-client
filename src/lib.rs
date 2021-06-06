#![feature(trait_alias)]
mod filter;

pub use filter::{Comparison, Filter, Order};

use hyper::{
    body::Buf,
    client::{connect::Connect, Client},
    http::uri::{Authority, InvalidUri, PathAndQuery, Scheme},
    Body, Response, Uri,
};
use log::debug;
use serde::{de::DeserializeOwned, Deserialize};
use std::{convert::TryFrom, io::Read};
use thiserror::Error;

pub trait Connector = Connect + Clone + Send + Sync + 'static;

pub struct DataSource<C>
where
    C: ,
{
    client: Client<C>,
    authority: Authority,
    scheme: Scheme,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid URI")]
    Uri(#[from] InvalidUri),
    #[error("http error")]
    Http(#[from] hyper::http::Error),
    #[error("hyper error")]
    Hyper(#[from] hyper::Error),
    #[error("serde error")]
    Serde(serde_json::Error, String),
    #[error("io error")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub value: Vec<T>,
    #[serde(rename = "odata.count")]
    pub count: Option<String>,
    #[serde(rename = "odata.nextLink")]
    pub next_link: Option<String>,
}

fn path(resource_type: &str, filter: &Filter) -> Result<PathAndQuery, InvalidUri> {
    format!(
        "/api/{resource_type}?{query}",
        resource_type = urlencoding::encode(resource_type),
        query = filter.to_query()
    )
    .parse()
}

impl<C> DataSource<C>
where
    C: Connector,
{
    pub fn new<A>(client: Client<C>, domain: A) -> Result<DataSource<C>, Error>
    where
        Authority: TryFrom<A>,
        Error: From<<Authority as TryFrom<A>>::Error>,
    {
        Ok(DataSource {
            client,
            authority: Authority::try_from(domain)?,
            scheme: Scheme::HTTPS,
        })
    }

    pub async fn get(
        &self,
        resource_type: &str,
        filter: Option<Filter>,
    ) -> Result<Response<Body>, Error> {
        let uri = Uri::builder()
            .scheme(self.scheme.as_ref())
            .authority(self.authority.as_ref())
            .path_and_query(path(resource_type, &filter.unwrap_or_default())?)
            .build()
            .map_err(Error::from)?;

        debug!("fetching {}", uri);
        Ok(self.client.get(uri).await?)
    }

    pub async fn get_as<T: DeserializeOwned>(
        &self,
        resource_type: &str,
        filter: Option<Filter>,
    ) -> Result<Page<T>, Error> {
        let response = self.get(resource_type, filter).await?;
        let body = hyper::body::aggregate(response).await?;

        let mut content = String::new();
        body.reader().read_to_string(&mut content)?;

        serde_json::from_str(&content).map_err(|e| Error::Serde(e, content))
    }
}
