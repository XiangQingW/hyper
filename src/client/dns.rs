use std::io;
use std::net::{SocketAddr, ToSocketAddrs, Ipv4Addr, SocketAddrV4};
use std::vec;
use std::collections::HashMap;
use std::sync::RwLock;

use ::futures::{Future, Poll};
use ::futures_cpupool::{CpuPool, CpuFuture};

#[derive(Clone)]
pub struct Dns {
    pool: CpuPool,
}

impl Dns {
    pub fn new(threads: usize) -> Dns {
        Dns {
            pool: CpuPool::new(threads)
        }
    }

    pub fn resolve(&self, host: String, port: u16) -> Query {
        Query(self.pool.spawn_fn(move || work(host, port)))
    }
}

pub struct Query(CpuFuture<IpAddrs, io::Error>);

impl Future for Query {
    type Item = IpAddrs;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0.poll()
    }
}

pub struct IpAddrs {
    iter: vec::IntoIter<SocketAddr>,
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

impl IpAddrs {
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

impl Iterator for IpAddrs {
    type Item = SocketAddr;
    #[inline]
    fn next(&mut self) -> Option<SocketAddr> {
        self.iter.next()
    }
}

pub type Answer = io::Result<IpAddrs>;

fn work(hostname: String, port: u16) -> Answer {
    debug!("resolve {:?}:{:?}", hostname, port);
    (&*hostname, port).to_socket_addrs().map(|i| IpAddrs { iter: i })
}
