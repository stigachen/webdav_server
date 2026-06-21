use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum ServerEvent {
    ClientConnected {
        peer: String,
    },
    RequestCompleted {
        method: String,
        path: String,
        status: u16,
        bytes_in: u64,
        bytes_out: u64,
        duration: Duration,
    },
    ServerStopped,
}

#[derive(Debug)]
pub struct EventBus {
    senders: Vec<Sender<ServerEvent>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            senders: Vec::new(),
        }
    }

    pub fn subscribe(&mut self) -> Receiver<ServerEvent> {
        let (sender, receiver) = mpsc::channel();
        self.senders.push(sender);
        receiver
    }

    pub fn emit(&mut self, event: ServerEvent) {
        self.senders
            .retain(|sender| sender.send(event.clone()).is_ok());
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ActivityEntry {
    pub method: String,
    pub path: String,
    pub status: u16,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub duration: Duration,
}

#[derive(Debug, Clone)]
struct Sample {
    at: Instant,
    bytes_in: u64,
    bytes_out: u64,
}

#[derive(Debug, Clone)]
pub struct Metrics {
    started_at: Instant,
    active_requests: u64,
    total_connections: u64,
    total_requests: u64,
    total_bytes_in: u64,
    total_bytes_out: u64,
    recent: VecDeque<ActivityEntry>,
    samples: VecDeque<Sample>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            active_requests: 0,
            total_connections: 0,
            total_requests: 0,
            total_bytes_in: 0,
            total_bytes_out: 0,
            recent: VecDeque::new(),
            samples: VecDeque::new(),
        }
    }

    pub fn apply(&mut self, event: ServerEvent) {
        match event {
            ServerEvent::ClientConnected { .. } => {
                self.active_requests += 1;
                self.total_connections += 1;
            }
            ServerEvent::RequestCompleted {
                method,
                path,
                status,
                bytes_in,
                bytes_out,
                duration,
            } => {
                self.active_requests = self.active_requests.saturating_sub(1);
                self.total_requests += 1;
                self.total_bytes_in += bytes_in;
                self.total_bytes_out += bytes_out;
                self.samples.push_back(Sample {
                    at: Instant::now(),
                    bytes_in,
                    bytes_out,
                });
                self.recent.push_front(ActivityEntry {
                    method,
                    path,
                    status,
                    bytes_in,
                    bytes_out,
                    duration,
                });
                while self.recent.len() > 8 {
                    self.recent.pop_back();
                }
            }
            ServerEvent::ServerStopped => {
                self.active_requests = 0;
            }
        }
        self.prune_samples();
    }

    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn active_requests(&self) -> u64 {
        self.active_requests
    }

    pub fn total_connections(&self) -> u64 {
        self.total_connections
    }

    pub fn total_requests(&self) -> u64 {
        self.total_requests
    }

    pub fn total_bytes_in(&self) -> u64 {
        self.total_bytes_in
    }

    pub fn total_bytes_out(&self) -> u64 {
        self.total_bytes_out
    }

    pub fn recent(&self) -> impl Iterator<Item = &ActivityEntry> {
        self.recent.iter()
    }

    pub fn transfer_rates(&self) -> (f64, f64) {
        let window = 5.0;
        let bytes_in = self
            .samples
            .iter()
            .map(|sample| sample.bytes_in)
            .sum::<u64>() as f64;
        let bytes_out = self
            .samples
            .iter()
            .map(|sample| sample.bytes_out)
            .sum::<u64>() as f64;
        (bytes_in / window, bytes_out / window)
    }

    fn prune_samples(&mut self) {
        let cutoff = Duration::from_secs(5);
        while self
            .samples
            .front()
            .is_some_and(|sample| sample.at.elapsed() > cutoff)
        {
            self.samples.pop_front();
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Metrics, ServerEvent};

    #[test]
    fn metrics_track_requests_and_clients() {
        let mut metrics = Metrics::new();
        metrics.apply(ServerEvent::ClientConnected {
            peer: "127.0.0.1:1234".to_string(),
        });
        assert_eq!(metrics.active_requests(), 1);
        assert_eq!(metrics.total_connections(), 1);

        metrics.apply(ServerEvent::RequestCompleted {
            method: "GET".to_string(),
            path: "/hello.txt".to_string(),
            status: 200,
            bytes_in: 0,
            bytes_out: 11,
            duration: Duration::from_millis(3),
        });

        assert_eq!(metrics.active_requests(), 0);
        assert_eq!(metrics.total_requests(), 1);
        assert_eq!(metrics.total_bytes_out(), 11);
        assert_eq!(metrics.recent().count(), 1);
    }
}
