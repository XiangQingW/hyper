use std::io;
use std::net::{SocketAddr, ToSocketAddrs, Ipv4Addr, SocketAddrV4, Ipv6Addr, SocketAddrV6};
use std::vec;
use std::collections::HashMap;
use std::sync::RwLock;

use ::futures::{Async, Future, Poll};
use Uri;

pub const RERANK_FRAGMENT: &'static str = "947afd3b7a_rerank";
pub const IP_FRAGMENT_PREFIX: &'static str = "430BB5C318_ip:";

pub struct Work {
    host: String,
    port: u16
}

impl Work {
    pub fn new(host: String, port: u16) -> Work {
        Work { host: host, port: port }
    }
}

impl Future for Work {
    type Item = IpAddrs;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        debug!("resolving host={:?}, port={:?}", self.host, self.port);
        (&*self.host, self.port).to_socket_addrs()
            .map(|i| Async::Ready(IpAddrs { iter: i }))
    }
}

#[derive(Clone, Debug)]
pub struct IpAddrs {
    pub iter: vec::IntoIter<SocketAddr>,
}

impl IpAddrs {
    pub fn try_parse(host: &str, port: u16) -> Option<IpAddrs> {
        if let Ok(addr) = host.parse::<Ipv4Addr>() {
            let addr = SocketAddrV4::new(addr, port);
            return Some(IpAddrs { iter: vec![SocketAddr::V4(addr)].into_iter() })
        }
        if let Ok(addr) = host.parse::<Ipv6Addr>() {
            let addr = SocketAddrV6::new(addr, port, 0, 0);
            return Some(IpAddrs { iter: vec![SocketAddr::V6(addr)].into_iter() })
        }
        None
    }

    pub fn new(addr: SocketAddr) -> Self {
        IpAddrs {
            iter: vec![addr].into_iter()
        }
    }
}

lazy_static! {
    static ref CUSTOM_DOMAIN2ADDR: RwLock<HashMap<String, SocketAddr>> = {
        let h = HashMap::new();
        RwLock::new(h)
    };
}

fn get_custom_addr(domain: &str) -> Option<SocketAddr> {
    match CUSTOM_DOMAIN2ADDR.read() {
        Ok(addrs) => addrs.get(domain).cloned(),
        _ => None,
    }
}

pub(crate) fn get_addrs_by_uri(uri: &Uri) -> Option<SocketAddr> {
    let fragment = uri.fragment()?;

    if !fragment.starts_with(IP_FRAGMENT_PREFIX) {
        return None;
    }

    let elements: Vec<_> = fragment.split(':').collect();
    let ip = elements.get(1)?;

    get_addr_by_ip(ip)
}

pub(crate) fn is_reranking(uri: &Uri) -> bool {
    match uri.fragment() {
        Some(fragment) if fragment == RERANK_FRAGMENT => true,
        _ => false,
    }
}

fn get_addr_by_ip(ip: &str) -> Option<SocketAddr> {
    match ip.parse::<Ipv4Addr>() {
        Ok(addr) => {
            let addr = SocketAddrV4::new(addr, 443);
            let addr = SocketAddr::V4(addr);
            Some(addr)
        },
        Err(err) => {
            warn!("get addr by ip failed: err= {:?} ip= {:?}", err, ip);
            None
        }
    }
}

pub fn set_custom_addr(domain: String, addr: &str) {
    if let Ok(mut addrs) = CUSTOM_DOMAIN2ADDR.write() {
        if let Some(addr) = get_addr_by_ip(addr) {
            addrs.insert(domain, addr);
        }
    }
}

pub fn remove_custom_addr(domain: &str) {
    if let Ok(mut addrs) = CUSTOM_DOMAIN2ADDR.write() {
        addrs.remove(domain);
    }
}

impl IpAddrs {
    pub fn try_parse_custom(domain: &str, is_reranking: bool) -> Option<IpAddrs> {
        if is_reranking {
            return None;
        }

        match get_custom_addr(domain) {
            Some(addr) => {
                debug!("get custom addr: domain= {:?} addr= {:?}", domain, addr);
                Some(IpAddrs::new(addr))
            },
            None => None,
        }
    }
}

impl Iterator for IpAddrs {
    type Item = SocketAddr;
    #[inline]
    fn next(&mut self) -> Option<SocketAddr> {
        self.iter.next()
    }
}
