use std::net::{IpAddr, Ipv4Addr, UdpSocket};

pub fn display_host(bind_host: &str) -> String {
    if bind_host != "0.0.0.0" {
        return bind_host.to_string();
    }
    lan_ip()
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .to_string()
}

fn lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}
