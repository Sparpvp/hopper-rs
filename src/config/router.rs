use std::{collections::HashMap, net::SocketAddr, ops::Deref};

use serde::{Deserialize, Deserializer};
use tokio::sync::Mutex;

use crate::server::{
    bridge::{Bridge, ForwardStrategy},
    router::RouterError,
    IncomingClient, Router,
};

use self::{balancer::Balanced, resolver::ResolvableAddr};

mod balancer;
mod resolver;

fn deserialize_mutex<'de, D, T: Deserialize<'de>>(deserializer: D) -> Result<Mutex<T>, D::Error>
where
    D: Deserializer<'de>,
{
    let inner = T::deserialize(deserializer)?;
    Ok(Mutex::new(inner))
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum RouteType {
    Simple(ResolvableAddr),
    #[serde(deserialize_with = "deserialize_mutex")]
    Balanced(Mutex<Balanced>),
}

impl RouteType {
    async fn get(&self) -> SocketAddr {
        match self {
            RouteType::Simple(route) => (*route).into(),
            RouteType::Balanced(balancer) => balancer.lock().await.get(),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct RouteInfo {
    #[serde(alias = "ip-forwarding", default)]
    ip_forwarding: ForwardStrategy,

    ip: RouteType,
}

#[derive(Deserialize, Debug)]
pub struct RouterConfig {
    default: Option<RouteInfo>,

    #[serde(default)]
    routes: HashMap<String, RouteInfo>,
}

#[async_trait::async_trait]
impl Router for RouterConfig {
    // type Error = ConfigRouterError;

    async fn route(&self, client: &mut IncomingClient) -> Result<Bridge, RouterError> {
        // resolve hostname from the configuration
        let route = self
            .routes
            .get(client.destination.deref())
            .or(self.default.as_ref())
            .ok_or(RouterError::NoServer)?;

        // connect to the resolved backend server
        Bridge::connect(route.ip.get().await, route.ip_forwarding)
            .await
            .map_err(RouterError::Unreachable)
    }
}
