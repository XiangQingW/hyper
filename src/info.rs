use std::time::Instant;
use std::net::SocketAddr;
use std::cell::RefCell;
use std::collections::HashMap;

task_local!(static SEND_REQ_TS: RefCell<HashMap<String, Instant>> = RefCell::new(HashMap::new()));

task_local!(static CONNECTION_BEGIN_TS: RefCell<HashMap<String, Instant>> = RefCell::new(HashMap::new()));
task_local!(static DNS_INFO: RefCell<HashMap<String, DnsInfo>> = RefCell::new(HashMap::new()));
task_local!(static TLS_INFO: RefCell<HashMap<String, TlsInfo>> = RefCell::new(HashMap::new()));
task_local!(static CONNECTION_INFO: RefCell<HashMap<String, ConnectionInfo>> = RefCell::new(HashMap::new()));

task_local!(static CONNECT_INFO: RefCell<HashMap<String, ConnectInfo>> = RefCell::new(HashMap::new()));

task_local!(static REQ_FINISHED_TS: RefCell<Option<Instant>> = RefCell::new(None));
task_local!(static RES_BEGIN_TS: RefCell<Option<Instant>> = RefCell::new(None));
task_local!(static RES_HEADER_FINISHED_TS: RefCell<Option<Instant>> = RefCell::new(None));
task_local!(static RES_HEADER_LENGTH: RefCell<Option<u64>> = RefCell::new(None));

task_local!(static TRANSPORT_INFO: RefCell<HashMap<String, TransportInfo>> = RefCell::new(HashMap::new()));

#[derive(Clone, Debug)]
pub struct TransportInfo {
    pub req_finished_ts: Instant,
    pub res_begin_ts: Instant,
    pub res_header_finished_ts: Instant,
    pub res_header_length: u64,
}

#[derive(Clone, Debug)]
pub struct ConnectionInfo {
    pub req_id: String,
    pub connection_begin_ts: Instant,
    pub dns_info: DnsInfo,
    pub tcp_info: TcpInfo,
    pub tls_info: Option<TlsInfo>,
}

#[derive(Clone, Debug)]
pub enum AddrResolveType {
    Ip,
    Specified,
    Cached,
    Resolved,
}

#[derive(Clone, Debug)]
pub struct DnsInfo {
    pub addr_resolve_type: AddrResolveType,
    pub finished_ts: Instant,
}

impl DnsInfo {
    pub(crate) fn new(addr_resolve_type: AddrResolveType) -> Self {
        Self {
            addr_resolve_type,
            finished_ts: Instant::now(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TcpInfo {
    pub pair_addrs: (SocketAddr, SocketAddr),
    #[cfg(unix)]
    pub raw_fd: std::os::unix::io::RawFd,
    pub finished_ts: Instant,
}

#[derive(Clone, Debug)]
pub struct TlsInfo {
    pub alpn: Option<String>,
    pub protocol_version: Option<String>,
    pub cipher_suite: Option<String>,
    pub finished_ts: Instant,
}

#[derive(Clone, Debug)]
pub struct ConnectInfo {
    pub consulted_alpn: String,
    pub is_proxied: bool,
    pub reuse_idle_connection: bool,
    pub send_req_ts: Instant,
    pub connect_finished_ts: Instant,
    pub connection_info: ConnectionInfo,
}

pub(crate) fn set_send_req_ts(req_id: String, ts: Instant) {
    SEND_REQ_TS.with(|s| s.borrow_mut().insert(req_id, ts));
}
pub fn get_send_req_ts(req_id: &str) -> Option<Instant> {
    SEND_REQ_TS.with(|s| s.borrow().get(req_id).cloned())
}

pub(crate) fn set_connection_begin_ts(req_id: String) {
    CONNECTION_BEGIN_TS.with(|s| s.borrow_mut().insert(req_id, Instant::now()));
}
pub fn get_connection_begin_ts(req_id: &str) -> Option<Instant> {
    CONNECTION_BEGIN_TS.with(|s| s.borrow().get(req_id).cloned())
}

pub(crate) fn set_dns_info(req_id: String, dns_info: DnsInfo) {
    DNS_INFO.with(|s| s.borrow_mut().insert(req_id, dns_info));
}
pub fn get_dns_info(req_id: &str) -> Option<DnsInfo> {
    DNS_INFO.with(|s| s.borrow().get(req_id).cloned())
}

pub fn set_tls_info(req_id: String, tls_info: TlsInfo) {
    TLS_INFO.with(|s| s.borrow_mut().insert(req_id, tls_info));
}
pub fn get_tls_info(req_id: &str) -> Option<TlsInfo> {
    TLS_INFO.with(|s| s.borrow().get(req_id).cloned())
}

pub(crate) fn set_connection_info(req_id: String, connection_info: ConnectionInfo) {
    CONNECTION_INFO.with(|s| s.borrow_mut().insert(req_id, connection_info));
}
pub fn get_connection_info(req_id: &str) -> Option<ConnectionInfo> {
    CONNECTION_INFO.with(|s| s.borrow().get(req_id).cloned())
}

pub(crate) fn set_connect_info(req_id: String, connect_info: ConnectInfo) {
    CONNECT_INFO.with(|s| s.borrow_mut().insert(req_id, connect_info));
}
pub fn get_connect_info(req_id: &str) -> Option<ConnectInfo> {
    CONNECT_INFO.with(|s| s.borrow().get(req_id).cloned())
}

pub(crate) fn set_req_finished_ts() {
    REQ_FINISHED_TS.with(|s| *s.borrow_mut() = Some(Instant::now()));
}
pub(crate) fn get_req_finished_ts() -> Option<Instant> {
    REQ_FINISHED_TS.with(|s| s.borrow().clone())
}

pub(crate) fn set_res_begin_ts() {
    RES_BEGIN_TS.with(|s| *s.borrow_mut() = Some(Instant::now()));
}
pub(crate) fn get_res_begin_ts() -> Option<Instant> {
    RES_BEGIN_TS.with(|s| s.borrow().clone())
}

pub(crate) fn set_res_header_finished_ts() {
    RES_HEADER_FINISHED_TS.with(|s| *s.borrow_mut() = Some(Instant::now()));
}
pub(crate) fn get_res_header_finished_ts() -> Option<Instant> {
    RES_HEADER_FINISHED_TS.with(|s| s.borrow().clone())
}

pub(crate) fn set_res_header_length(len: u64) {
    RES_HEADER_LENGTH.with(|s| *s.borrow_mut() = Some(len));
}
pub(crate) fn get_res_header_length() -> Option<u64> {
    RES_HEADER_LENGTH.with(|s| s.borrow().clone())
}

pub(crate) fn set_transport_info(req_id: String, transport_info: TransportInfo) {
    TRANSPORT_INFO.with(|s| s.borrow_mut().insert(req_id, transport_info));
}
pub fn get_transport_info(req_id: &str) -> Option<TransportInfo> {
    TRANSPORT_INFO.with(|s| s.borrow().get(req_id).cloned())
}

pub const DEFAULT_REQ_ID: &str = "default";
pub fn get_uri_req_id(uri: &crate::Uri) -> String {
    let req_id = if let Some(fragment) = uri.fragment() {
        let elements: Vec<_> = fragment.split('+').collect();
        elements.last().map(|s| s.to_string())
    } else {
        None
    };

    req_id.unwrap_or_else(|| {
        debug!("uri don't have req_id: uri= {:?}", uri);
        DEFAULT_REQ_ID.to_owned()
    })
}
