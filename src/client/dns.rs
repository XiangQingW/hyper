use std::io;
use std::net::{SocketAddr, ToSocketAddrs, Ipv4Addr, SocketAddrV4, Ipv6Addr, SocketAddrV6};
use std::vec;
use std::collections::HashMap;
use std::sync::RwLock;

use ::futures::{Async, Future, Poll};

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

pub struct IpAddrs {
    iter: vec::IntoIter<SocketAddr>,
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

    pub fn try_parse_custom(domain: &str) -> Option<IpAddrs> {
        match get_custom_addr(domain) {
            Some(addr) => {
                debug!("get custom addr: domain= {:?} addr= {:?}", domain, addr);
                Some(IpAddrs {
                    iter: vec![addr].into_iter()
                })
            },
            None => None,
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

pub fn set_custom_addr(domain: String, addr: &str) {
    if let Ok(mut addrs) = CUSTOM_DOMAIN2ADDR.write() {
        if let Ok(addr) = addr.parse::<Ipv4Addr>() {
            let addr = SocketAddrV4::new(addr, 443);
            let addr = SocketAddr::V4(addr);
            addrs.insert(domain, addr);
        }
    }
}

pub fn remove_custom_addr(domain: &str) {
    if let Ok(mut addrs) = CUSTOM_DOMAIN2ADDR.write() {
        addrs.remove(domain);
    }
}

impl Iterator for IpAddrs {
    type Item = SocketAddr;
    #[inline]
    fn next(&mut self) -> Option<SocketAddr> {
        self.iter.next()
    }
}
