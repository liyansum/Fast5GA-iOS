//! Retry an unreachable on-device destination on its matching VPN interface.
//! Socket scoping affects this socket only; it does not modify system routes.
use std::{io, net::{Ipv4Addr, SocketAddr}};
use tokio::net::TcpStream;

#[derive(Clone, Debug)]
pub struct VpnInterface {
    pub name: String,
    pub index: u32,
    pub address: Ipv4Addr,
    pub netmask: Ipv4Addr,
}

pub fn matching_interfaces(target: Ipv4Addr, interfaces: &[VpnInterface]) -> Vec<VpnInterface> {
    let mut matches = interfaces.iter().filter_map(|interface| {
        if interface.index == 0 || !(interface.name.starts_with("utun") || interface.name.starts_with("ipsec")) {
            return None;
        }
        let target = u32::from(target);
        let local = u32::from(interface.address);
        let mask = u32::from(interface.netmask);
        let rank = if target == local { 128 }
            else if mask != 0 && target & mask == local & mask { 64 + mask.count_ones() }
            // LocalDevVPN can assign /32 addresses to adjacent tunnel peers.
            else if target & 0xffffff00 == local & 0xffffff00 { 24 }
            else { return None };
        Some((rank, interface.clone()))
    }).collect::<Vec<_>>();
    matches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.index.cmp(&b.1.index)));
    matches.into_iter().map(|(_, interface)| interface).collect()
}

pub fn is_route_error(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(code) if code == libc::ENETUNREACH || code == libc::EHOSTUNREACH || code == libc::ENETDOWN)
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
pub fn vpn_interfaces() -> Vec<VpnInterface> {
    unsafe {
        let mut first: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut first) != 0 { return vec![]; }
        let mut interfaces = vec![];
        let mut cursor = first;
        while !cursor.is_null() {
            let item = &*cursor;
            cursor = item.ifa_next;
            if item.ifa_addr.is_null() || item.ifa_netmask.is_null() || item.ifa_name.is_null()
                || item.ifa_flags & libc::IFF_UP as u32 == 0
                || (*item.ifa_addr).sa_family as i32 != libc::AF_INET { continue; }
            let name = std::ffi::CStr::from_ptr(item.ifa_name).to_string_lossy().into_owned();
            if !name.starts_with("utun") && !name.starts_with("ipsec") { continue; }
            let address = &*(item.ifa_addr as *const libc::sockaddr_in);
            let mask = &*(item.ifa_netmask as *const libc::sockaddr_in);
            interfaces.push(VpnInterface {
                index: libc::if_nametoindex(item.ifa_name), name,
                address: Ipv4Addr::from(u32::from_be(address.sin_addr.s_addr)),
                netmask: Ipv4Addr::from(u32::from_be(mask.sin_addr.s_addr)),
            });
        }
        libc::freeifaddrs(first);
        interfaces
    }
}

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
pub fn vpn_interfaces() -> Vec<VpnInterface> { vec![] }

/// Shared by the initial RPPairing socket and the subsequent TLS tunnel socket.
pub async fn connect(addr: SocketAddr) -> io::Result<TcpStream> {
    let original = match TcpStream::connect(addr).await {
        Ok(stream) => return Ok(stream),
        Err(error) => error,
    };
    if !is_route_error(&original) { return Err(original); }
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    if let std::net::IpAddr::V4(target) = addr.ip() {
        // Re-enumerate on every route failure: airplane/VPN changes renumber utun.
        let candidates = matching_interfaces(target, &vpn_interfaces());
        let mut results = vec![];
        for interface in candidates {
            match tokio::time::timeout(std::time::Duration::from_secs(2), connect_bound(addr, interface.index)).await {
                Ok(Ok(stream)) => {
                    tracing::info!("VPN scoped dial succeeded: {addr} via {} (index {})", interface.name, interface.index);
                    return Ok(stream);
                }
                Ok(Err(error)) => results.push(format!("{}#{}: {error}", interface.name, interface.index)),
                Err(_) => results.push(format!("{}#{}: scoped connection timed out after 2s", interface.name, interface.index)),
            }
        }
        let detail = if results.is_empty() { "no matching active IPv4 VPN interface".into() } else { results.join("; ") };
        return Err(io::Error::new(original.kind(), format!("system route: {original}; VPN interface retry: {detail}")));
    }
    Err(original)
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
pub async fn connect_bound(addr: SocketAddr, index: u32) -> io::Result<TcpStream> {
    use std::os::fd::AsRawFd;
    if index == 0 || !addr.is_ipv4() { return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid IPv4 interface scope")); }
    let socket = tokio::net::TcpSocket::new_v4()?;
    let rc = unsafe {
        libc::setsockopt(socket.as_raw_fd(), libc::IPPROTO_IP, libc::IP_BOUND_IF,
            &index as *const u32 as *const libc::c_void, std::mem::size_of_val(&index) as libc::socklen_t)
    };
    if rc != 0 { return Err(io::Error::last_os_error()); }
    socket.connect(addr).await
}
