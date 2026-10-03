//! Fire-and-forget DogStatsD over UDP. Sends never block or fail the sync.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

pub struct Metrics {
    target: Option<(UdpSocket, SocketAddr)>,
}

impl Metrics {
    /// Sends to `addr` (`host:port`), or drops every metric when `addr` is
    /// `None` or cannot be resolved.
    pub fn new(addr: Option<&str>) -> Self {
        let target = addr.and_then(|addr| {
            let addr = addr.to_socket_addrs().ok()?.next()?;
            let bind = if addr.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            };
            let socket = UdpSocket::bind(bind).ok()?;
            socket.set_nonblocking(true).ok()?;
            Some((socket, addr))
        });
        if addr.is_some() && target.is_none() {
            eprintln!("sentry-options-sync: cannot send metrics to {addr:?}; metrics disabled");
        }
        Self { target }
    }

    pub fn distribution(&self, name: &str, value: f64, tags: &[(&str, &str)]) {
        if let Some((socket, addr)) = &self.target {
            let _ = socket.send_to(format_distribution(name, value, tags).as_bytes(), addr);
        }
    }
}

fn format_distribution(name: &str, value: f64, tags: &[(&str, &str)]) -> String {
    let mut line = format!("{name}:{value}|d");
    for (i, (key, value)) in tags.iter().enumerate() {
        line.push_str(if i == 0 { "|#" } else { "," });
        line.push_str(key);
        line.push(':');
        line.push_str(value);
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn formats_distribution() {
        assert_eq!(
            format_distribution("a.b", 1.5, &[("x", "1"), ("y", "2")]),
            "a.b:1.5|d|#x:1,y:2"
        );
        assert_eq!(format_distribution("a.b", 2.0, &[]), "a.b:2|d");
    }

    #[test]
    fn sends_to_statsd() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let metrics = Metrics::new(Some(&server.local_addr().unwrap().to_string()));

        metrics.distribution("a.b", 0.25, &[("x", "1")]);

        let mut buf = [0; 64];
        let len = server.recv(&mut buf).unwrap();
        assert_eq!(&buf[..len], b"a.b:0.25|d|#x:1");
    }

    #[test]
    fn disabled_without_address() {
        Metrics::new(None).distribution("a.b", 1.0, &[]);
    }
}
