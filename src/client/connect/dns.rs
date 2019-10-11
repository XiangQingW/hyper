//! The `Resolve` trait, support types, and some basic implementations.
//!
//! This module contains:
//!
//! - A [`GaiResolver`](GaiResolver) that is the default resolver for the
//!   `HttpConnector`.
//! - The [`Resolve`](Resolve) trait and related types to build a custom
//!   resolver for use with the `HttpConnector`.
#![allow(missing_docs)]
use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashSet},
    fmt,
    hash::{Hash, Hasher},
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs},
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
    vec
};

use futures::{
    future::{ExecuteError, Executor},
    sync::oneshot,
    Async, Future, Poll
};
use futures_cpupool::Builder as CpuPoolBuilder;
use tokio_threadpool;

use self::sealed::GaiTask;

/// Resolve a hostname to a set of IP addresses.
pub trait Resolve {
    /// The set of IP addresses to try to connect to.
    type Addrs: Iterator<Item = IpAddr>;
    /// A Future of the resolved set of addresses.
    type Future: Future<Item = Self::Addrs, Error = io::Error>;
    /// Resolve a hostname.
    fn resolve(&self, name: Name) -> Self::Future;
}

/// A domain name to resolve into IP addresses.
pub struct Name {
    host: String
}

/// A resolver using blocking `getaddrinfo` calls in a threadpool.
#[derive(Clone)]
pub struct GaiResolver {
    executor: GaiExecutor
}

/// An iterator of IP addresses returned from `getaddrinfo`.
pub struct GaiAddrs {
    inner: IpAddrs
}

/// A future to resole a name returned by `GaiResolver`.
pub struct GaiFuture {
    rx: oneshot::SpawnHandle<IpAddrs, io::Error>
}

impl Name {
    pub(super) fn new(host: String) -> Name {
        Name { host }
    }

    /// View the hostname as a string slice.
    pub fn as_str(&self) -> &str {
        &self.host
    }
}

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.host, f)
    }
}

impl GaiResolver {
    /// Construct a new `GaiResolver`.
    ///
    /// Takes number of DNS worker threads.
    pub fn new(threads: usize) -> Self {
        let pool = CpuPoolBuilder::new()
            .name_prefix("hyper-dns")
            .pool_size(threads)
            .create();
        GaiResolver::new_with_executor(pool)
    }

    /// Construct a new `GaiResolver` with a shared thread pool executor.
    ///
    /// Takes an executor to run blocking `getaddrinfo` tasks on.
    pub fn new_with_executor<E: 'static>(executor: E) -> Self
    where
        E: Executor<GaiTask> + Send + Sync
    {
        GaiResolver {
            executor: GaiExecutor(Arc::new(executor))
        }
    }
}

impl Resolve for GaiResolver {
    type Addrs = GaiAddrs;
    type Future = GaiFuture;

    fn resolve(&self, name: Name) -> Self::Future {
        let blocking = GaiBlocking::new(name.host);
        let rx = oneshot::spawn(blocking, &self.executor);
        GaiFuture { rx }
    }
}

impl fmt::Debug for GaiResolver {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad("GaiResolver")
    }
}

impl Future for GaiFuture {
    type Error = io::Error;
    type Item = GaiAddrs;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let addrs = try_ready!(self.rx.poll());
        Ok(Async::Ready(GaiAddrs { inner: addrs }))
    }
}

impl fmt::Debug for GaiFuture {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad("GaiFuture")
    }
}

impl Iterator for GaiAddrs {
    type Item = IpAddr;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|sa| sa.ip())
    }
}

impl fmt::Debug for GaiAddrs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad("GaiAddrs")
    }
}

#[derive(Clone)]
struct GaiExecutor(Arc<Executor<GaiTask> + Send + Sync>);

impl Executor<oneshot::Execute<GaiBlocking>> for GaiExecutor {
    fn execute(
        &self,
        future: oneshot::Execute<GaiBlocking>
    ) -> Result<(), ExecuteError<oneshot::Execute<GaiBlocking>>> {
        self.0
            .execute(GaiTask { work: future })
            .map_err(|err| ExecuteError::new(err.kind(), err.into_future().work))
    }
}

pub(super) struct GaiBlocking {
    host: String
}

impl GaiBlocking {
    pub(super) fn new(host: String) -> GaiBlocking {
        GaiBlocking { host }
    }
}

impl Future for GaiBlocking {
    type Error = io::Error;
    type Item = IpAddrs;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        debug!("resolving host={:?}", self.host);
        (&*self.host, 0)
            .to_socket_addrs()
            .map(|i| Async::Ready(IpAddrs { iter: i }))
    }
}

#[derive(Clone, Debug)]
pub(super) struct IpAddrs {
    iter: vec::IntoIter<SocketAddr>
}

impl IpAddrs {
    pub(super) fn new(addrs: Vec<SocketAddr>) -> Self {
        IpAddrs {
            iter: addrs.into_iter()
        }
    }

    pub(super) fn try_parse(host: &str, port: u16) -> Option<IpAddrs> {
        if let Ok(addr) = host.parse::<Ipv4Addr>() {
            let addr = SocketAddrV4::new(addr, port);
            return Some(IpAddrs {
                iter: vec![SocketAddr::V4(addr)].into_iter()
            });
        }
        if let Ok(addr) = host.parse::<Ipv6Addr>() {
            let addr = SocketAddrV6::new(addr, port, 0, 0);
            return Some(IpAddrs {
                iter: vec![SocketAddr::V6(addr)].into_iter()
            });
        }
        None
    }

    pub(super) fn split_by_preference(self) -> (IpAddrs, IpAddrs) {
        let preferring_v6 = self
            .iter
            .as_slice()
            .first()
            .map(SocketAddr::is_ipv6)
            .unwrap_or(false);

        let (preferred, fallback) = self
            .iter
            .partition::<Vec<_>, _>(|addr| addr.is_ipv6() == preferring_v6);

        (IpAddrs::new(preferred), IpAddrs::new(fallback))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.iter.as_slice().is_empty()
    }
}

impl Iterator for IpAddrs {
    type Item = SocketAddr;

    #[inline]
    fn next(&mut self) -> Option<SocketAddr> {
        self.iter.next()
    }
}

// Make this Future unnameable outside of this crate.
pub(super) mod sealed {
    use super::*;
    // Blocking task to be executed on a thread pool.
    pub struct GaiTask {
        pub(super) work: oneshot::Execute<GaiBlocking>
    }

    impl fmt::Debug for GaiTask {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.pad("GaiTask")
        }
    }

    impl Future for GaiTask {
        type Error = ();
        type Item = ();

        fn poll(&mut self) -> Poll<(), ()> {
            self.work.poll()
        }
    }
}

/// A resolver using `getaddrinfo` calls via the `tokio_threadpool::blocking`
/// API.
///
/// Unlike the `GaiResolver` this will not spawn dedicated threads, but only
/// works when running on the multi-threaded Tokio runtime.
#[derive(Clone, Debug)]
pub struct TokioThreadpoolGaiResolver(());

/// The future returned by `TokioThreadpoolGaiResolver`.
#[derive(Debug)]
pub struct TokioThreadpoolGaiFuture {
    name: Name
}

impl TokioThreadpoolGaiResolver {
    /// Creates a new DNS resolver that will use tokio threadpool's blocking
    /// feature.
    ///
    /// **Requires** its futures to be run on the threadpool runtime.
    pub fn new() -> Self {
        TokioThreadpoolGaiResolver(())
    }
}

impl Resolve for TokioThreadpoolGaiResolver {
    type Addrs = GaiAddrs;
    type Future = TokioThreadpoolGaiFuture;

    fn resolve(&self, name: Name) -> TokioThreadpoolGaiFuture {
        TokioThreadpoolGaiFuture { name }
    }
}

impl Future for TokioThreadpoolGaiFuture {
    type Error = io::Error;
    type Item = GaiAddrs;

    fn poll(&mut self) -> Poll<GaiAddrs, io::Error> {
        match tokio_threadpool::blocking(|| (self.name.as_str(), 0).to_socket_addrs()) {
            Ok(Async::Ready(Ok(iter))) => Ok(Async::Ready(GaiAddrs {
                inner: IpAddrs { iter }
            })),
            Ok(Async::Ready(Err(e))) => Err(e),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e))
        }
    }
}

use http::Uri;
use std::collections::HashMap;

pub const RERANK_FRAGMENT: &str = "947afd3b7a_rerank";
pub const IP_FRAGMENT_PREFIX: &str = "430BB5C318_ip:";

/// RW lock trait
trait RW<T> {
    fn write_lock(&self) -> RwLockWriteGuard<T>;
    fn read_lock(&self) -> RwLockReadGuard<T>;
}

impl<T> RW<T> for RwLock<T> {
    fn write_lock(&self) -> RwLockWriteGuard<T> {
        self.write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn read_lock(&self) -> RwLockReadGuard<T> {
        self.read().unwrap_or_else(|p| p.into_inner())
    }
}

/// addr source
#[derive(Debug, Hash, Eq, PartialEq, PartialOrd, Ord, Clone, Copy)]
pub enum AddrSource {
    HttpDNS = 0,
    LocalDNS,
    HardCodeIp
}

/// sorted addr
#[derive(Debug, Eq, PartialEq, PartialOrd, Clone)]
pub struct SortedAddr {
    addr: IpAddr,
    source: AddrSource,
    connect_costs: Vec<i32>
}

impl Hash for SortedAddr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.addr.hash(state);
        state.finish();
    }
}

impl SortedAddr {
    pub fn new(addr: IpAddr, source: AddrSource) -> Self {
        SortedAddr {
            addr,
            source,
            connect_costs: Vec::new()
        }
    }

    fn avg_cost(&self) -> i32 {
        if self.connect_costs.is_empty() {
            return std::i32::MAX;
        }

        let sum: i32 = self.connect_costs.iter().sum();
        sum / (self.connect_costs.len() as i32)
    }

    fn has_been_used(&self) -> bool {
        !self.connect_costs.is_empty()
    }

    fn delay_time(&self) -> i32 {
        if self.connect_costs.is_empty() {
            return 300;
        }

        std::cmp::min(600, self.avg_cost() + 250)
    }
}

impl Ord for SortedAddr {
    fn cmp(&self, other: &SortedAddr) -> Ordering {
        if self.addr == other.addr {
            return Ordering::Equal;
        }

        let self_avg_cost = self.avg_cost();
        let other_avg_cost = other.avg_cost();

        if self_avg_cost != other_avg_cost {
            return self_avg_cost.cmp(&other_avg_cost);
        }

        if self.source != other.source {
            return self.source.cmp(&other.source);
        }

        self.addr.cmp(&other.addr)
    }
}

lazy_static! {
    static ref CUSTOM_DOMAIN2ADDR: RwLock<HashMap<String, SocketAddr>> = {
        let h = HashMap::new();
        RwLock::new(h)
    };
    static ref DOMAIN2SORTED_ADDRS: RwLock<HashMap<String, BTreeSet<SortedAddr>>> =
        { RwLock::new(HashMap::new()) };
}

fn remove_old_domain_sorted_addrs(domain: &String, source: AddrSource) -> HashSet<SortedAddr> {
    let mut domain2addrs = DOMAIN2SORTED_ADDRS.write_lock();

    let addrs = match domain2addrs.get_mut(domain) {
        Some(addrs) => addrs,
        None => return HashSet::new()
    };

    let removed_addrs: HashSet<SortedAddr> = addrs
        .iter()
        .filter(|a| a.source == source)
        .cloned()
        .collect();

    for addr in &removed_addrs {
        addrs.remove(addr);
    }

    removed_addrs
}

fn take_sorted_addr(addrs: &mut BTreeSet<SortedAddr>, addr: &SortedAddr) -> Option<SortedAddr> {
    let mut res = None;
    for a in addrs.iter() {
        if a.addr == addr.addr {
            res = Some(a.clone());
            break;
        }
    }

    match res {
        Some(r) => addrs.take(&r),
        None => None
    }
}

/// insert domain sorted addrs
pub fn insert_domain_sorted_addrs(
    domain: String,
    mut sorted_addrs: Vec<SortedAddr>,
    source: AddrSource
) {
    let mut old_addrs = remove_old_domain_sorted_addrs(&domain, source);

    sorted_addrs.sort_by(|a, b| a.addr.cmp(&b.addr));
    let sorted_addrs: Vec<_> = sorted_addrs.into_iter().take(4).collect();

    let mut domain2addrs = DOMAIN2SORTED_ADDRS.write_lock();
    let entry = domain2addrs
        .entry(domain.clone())
        .or_insert_with(BTreeSet::new);

    for addr in sorted_addrs {
        if let Some(old_addr) = old_addrs.take(&addr) {
            entry.insert(old_addr);
            continue;
        }

        let a = match take_sorted_addr(entry, &addr) {
            Some(old_entry) => old_entry,
            None => addr
        };
        entry.insert(a);
    }

    debug!(
        "insert domain sorted addrs success: domain= {} entry= {:?} len= {:?} source= {:?}",
        domain,
        entry,
        entry.len(),
        source
    );
}

/// update domain sorted addr cost
pub fn update_domain_sorted_addr_cost(domain: &str, addr: IpAddr, cost_ms: i32) {
    let mut domain2addrs = DOMAIN2SORTED_ADDRS.write_lock();
    let addrs = match domain2addrs.get_mut(domain) {
        Some(addrs) => addrs,
        None => {
            warn!("domain sorted addr not found: domain= {}", domain);
            return;
        }
    };

    let mut sorted_addr = None;
    for a in addrs.iter() {
        if a.addr == addr {
            sorted_addr = Some(a.clone());
            break;
        }
    }

    let sorted_addr = match sorted_addr {
        Some(a) => a,
        None => {
            warn!(
                "addr not found in sorted addrs: addr= {:?} addrs= {:?}",
                addr, addrs
            );
            return;
        }
    };

    let mut addr = match addrs.take(&sorted_addr) {
        Some(a) => a,
        None => {
            warn!(
                "take addr not found in sorted addrs: addr= {:?} addrs= {:?}",
                addr, addrs
            );
            return;
        }
    };
    addr.connect_costs.push(cost_ms);
    if 3 < addr.connect_costs.len() {
        addr.connect_costs.remove(0);
    }

    addrs.insert(addr);
    debug!(
        "update domain sorted addr cost success: domain= {} cost_ms= {} addrs= {:?}",
        domain, cost_ms, addrs
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SocketAddrWithDelayTime {
    pub addr: SocketAddr,
    pub delay_time: i32,
    pub source: AddrSource
}

impl SocketAddrWithDelayTime {
    fn from_sorted_addr(sorted_addr: &SortedAddr, port: u16) -> Self {
        SocketAddrWithDelayTime {
            addr: SocketAddr::new(sorted_addr.addr.clone(), port),
            delay_time: sorted_addr.delay_time(),
            source: sorted_addr.source
        }
    }

    fn new(addr: SocketAddr, delay_time: i32) -> Self {
        SocketAddrWithDelayTime {
            addr,
            delay_time,
            source: AddrSource::LocalDNS
        }
    }
}

/// get sorted addrs
pub fn get_sorted_addrs(domain: &str, first_addr: SocketAddr) -> Vec<SocketAddrWithDelayTime> {
    let port = first_addr.port();
    let first_addr = SocketAddrWithDelayTime::new(first_addr, 0);

    let domain2addrs = DOMAIN2SORTED_ADDRS.read_lock();

    let addrs = match domain2addrs.get(domain) {
        Some(addrs) => addrs,
        None => return vec![first_addr]
    };

    if addrs.is_empty() {
        return vec![first_addr];
    }

    let mut sorted_addrs = vec![first_addr.clone()];

    let fastest_addr = addrs.iter().nth(0).unwrap();
    sorted_addrs.push(SocketAddrWithDelayTime::from_sorted_addr(
        fastest_addr,
        port
    ));

    if 1 < addrs.len() {
        let faster_addr = addrs.iter().nth(1).unwrap();
        sorted_addrs.push(SocketAddrWithDelayTime::from_sorted_addr(faster_addr, port));
    } else {
        sorted_addrs.push(sorted_addrs[0].clone());
    }

    fn get_delay_time(delay_time: i32, min: i32, max: i32) -> i32 {
        let t = std::cmp::max(min, delay_time);
        std::cmp::min(max, t)
    }

    let mut base_delay_time = 0;
    for (index, addr) in sorted_addrs.iter_mut().enumerate() {
        base_delay_time = std::cmp::max(base_delay_time, addr.delay_time);
        if index == 0 {
            addr.delay_time = 0;
            continue;
        }

        let factor = index as i32;
        addr.delay_time = get_delay_time(base_delay_time * factor, 300 * factor, 600 * factor);
    }

    info!(
        "get sorted addrs: {:?} first_addr= {:?}",
        sorted_addrs, first_addr
    );
    sorted_addrs
}

fn get_custom_addr(domain: &str) -> Option<SocketAddr> {
    match CUSTOM_DOMAIN2ADDR.read() {
        Ok(addrs) => addrs.get(domain).cloned(),
        _ => None
    }
}

pub(super) fn get_addrs_by_uri(uri: &Uri) -> Option<SocketAddr> {
    let fragment = uri.fragment()?;

    if !fragment.starts_with(IP_FRAGMENT_PREFIX) {
        return None;
    }

    let ip_fragment = fragment.split(crate::info::SPLIT_PAT).next()?;
    let elements: Vec<_> = ip_fragment.split(':').collect();
    let ip = elements.get(1)?;

    let port = uri
        .scheme_part()
        .map(|scheme| if scheme.as_str() == "http" { 80 } else { 443 });

    get_addr_by_ip(ip, port)
}

fn get_addr_by_ip(ip: &str, port: Option<u16>) -> Option<SocketAddr> {
    match ip.parse::<Ipv4Addr>() {
        Ok(addr) => {
            let addr = SocketAddrV4::new(addr, port.unwrap_or_else(|| 443));
            let addr = SocketAddr::V4(addr);
            Some(addr)
        }
        Err(err) => {
            warn!("get addr by ip failed: err= {:?} ip= {:?}", err, ip);
            None
        }
    }
}

pub fn set_custom_addr(domain: String, addr: &str) {
    if let Ok(mut addrs) = CUSTOM_DOMAIN2ADDR.write() {
        if let Some(addr) = get_addr_by_ip(addr, None) {
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
    pub fn try_parse_custom(domain: &str, port: u16) -> Option<IpAddrs> {
        get_custom_addr(domain).map(|mut addr| {
            debug!(
                "get custom addr: domain= {:?} addr= {:?} port= {:?}",
                domain, addr, port
            );

            if addr.port() != port {
                debug!(
                    "modify addr port: domain= {:?} addr= {:?} port= {:?}",
                    domain, addr, port
                );
                addr.set_port(port);
            }

            IpAddrs::new(vec![addr])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_ip_addrs_split_by_preference() {
        let v4_addr = (Ipv4Addr::new(127, 0, 0, 1), 80).into();
        let v6_addr = (Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1), 80).into();

        let (mut preferred, mut fallback) = IpAddrs {
            iter: vec![v4_addr, v6_addr].into_iter()
        }
        .split_by_preference();
        assert!(preferred.next().unwrap().is_ipv4());
        assert!(fallback.next().unwrap().is_ipv6());

        let (mut preferred, mut fallback) = IpAddrs {
            iter: vec![v6_addr, v4_addr].into_iter()
        }
        .split_by_preference();
        assert!(preferred.next().unwrap().is_ipv6());
        assert!(fallback.next().unwrap().is_ipv4());
    }
}
