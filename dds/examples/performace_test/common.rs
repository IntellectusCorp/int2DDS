use clap::Parser;
use int2dds::{
    core::time::Duration,
    infrastructure::qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
    topic::type_support::DdsType,
};

use std::fmt;

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
pub struct PerformanceTestData {
    pub seq_num: u64,
    pub timestamp: u64,
    pub data: Vec<u8>,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
pub struct LatencyTestData {
    pub seq_num: u64,
    pub send_timestamp: u64,
    pub echo_timestamp: u64,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub enum ReliabilityMode {
    BestEffort,
    Reliable,
}

impl fmt::Display for ReliabilityMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReliabilityMode::BestEffort => write!(f, "best"),
            ReliabilityMode::Reliable => write!(f, "reliable"),
        }
    }
}

impl From<&str> for ReliabilityMode {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "best" | "besteffort" | "best_effort" => ReliabilityMode::BestEffort,
            "reliable" => ReliabilityMode::Reliable,
            _ => ReliabilityMode::BestEffort,
        }
    }
}

impl ReliabilityMode {
    pub fn to_qos_policy(&self) -> ReliabilityQosPolicy {
        match self {
            ReliabilityMode::BestEffort => ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::BestEffort,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
            ReliabilityMode::Reliable => ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct CommonArgs {
    #[arg(long, default_value = "30")]
    pub execution_time: u64,

    #[arg(long, default_value = "1024")]
    pub data_len: usize,

    #[arg(long, default_value = "best")]
    pub reliability: String,

    #[arg(long, default_value = "10")]
    pub domain_id: i32,

    #[arg(long, default_value = "throughput")]
    pub test_mode: String,

    #[arg(long)]
    pub hz: Option<f64>,

    #[arg(long, default_value = "1")]
    pub warmup_time: u64,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct PublisherArgs {
    #[command(flatten)]
    pub common: CommonArgs,

    #[arg(long, default_value = "8192")]
    pub batch_size: usize,

    #[arg(long)]
    pub latency_count: Option<u64>,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct SubscriberArgs {
    #[command(flatten)]
    pub common: CommonArgs,

    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub out_of_order: bool,
}

pub struct PerformanceStats {
    pub total_samples: u64,
    pub lost_samples: u64,
    pub bytes_received: u64,
    pub start_time: std::time::Instant,
    pub end_time: std::time::Instant,
    pub latency_samples: Vec<f64>,
    pub seen_seqs: std::collections::HashSet<u64>,
}

impl PerformanceStats {
    pub fn new() -> Self {
        Self {
            total_samples: 0,
            lost_samples: 0,
            bytes_received: 0,
            start_time: std::time::Instant::now(),
            end_time: std::time::Instant::now(),
            latency_samples: Vec::new(),
            seen_seqs: std::collections::HashSet::new(),
        }
    }

    pub fn add_sample(&mut self, data_size: usize) {
        self.total_samples += 1;
        self.bytes_received += data_size as u64;
    }

    pub fn add_latency_sample(&mut self, latency_ns: u64) {
        self.latency_samples.push(latency_ns as f64); // Keep as ns
    }

    pub fn calculate_throughput(&self) -> (f64, f64) {
        let duration_secs = self.end_time.duration_since(self.start_time).as_secs_f64();
        if duration_secs > 0.0 {
            let msgs_per_sec = self.total_samples as f64 / duration_secs;
            let mbps = (self.bytes_received as f64 * 8.0) / (duration_secs * 1_000_000.0);
            (msgs_per_sec, mbps)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn calculate_loss_rate(&self, expected_samples: u64) -> f64 {
        if expected_samples > 0 {
            self.lost_samples as f64 / expected_samples as f64
        } else {
            0.0
        }
    }

    pub fn calculate_latency_stats(&self) -> (f64, f64, f64) {
        if self.latency_samples.is_empty() {
            return (0.0, 0.0, 0.0);
        }

        let sum: f64 = self.latency_samples.iter().sum();
        let avg = sum / self.latency_samples.len() as f64;

        let max = self.latency_samples.iter().cloned().fold(0.0_f64, f64::max);
        let min = self.latency_samples.iter().cloned().fold(f64::INFINITY, f64::min);

        (avg / 2.0, min / 2.0, max / 2.0)
    }

    pub fn calculate_lost_samples_from_hashset(&mut self) {
        if self.seen_seqs.is_empty() {
            return;
        }

        let max_seq = *self.seen_seqs.iter().max().unwrap();
        let expected_count = max_seq + 1;
        self.lost_samples = expected_count - self.seen_seqs.len() as u64;
    }

    pub fn print_summary(&mut self, out_of_order: bool) {
        if out_of_order {
            self.calculate_lost_samples_from_hashset();
        }
        let duration_secs = self.end_time.duration_since(self.start_time).as_secs_f64();
        let (msgs_per_sec, mbps) = self.calculate_throughput();

        println!("\n=== Performance Test Results ===");
        println!("Test duration: {:.2} seconds", duration_secs);
        println!("[INFO] Total samples received: {}", self.total_samples);
        println!("[INFO] Messages per second: {:.2}", msgs_per_sec);
        println!("Throughput: {:.2} Mbps", mbps);

        if let Some(&max_seq) = self.seen_seqs.iter().max() {
            let loss_rate = self.calculate_loss_rate(max_seq);
            println!("Last sequence number: {}", max_seq);
            if self.lost_samples > 0 {
                println!("[INFO] Lost samples: {}", self.lost_samples);
            }
            println!("[INFO] Loss rate: {:.2}%", loss_rate * 100.0);
        }

        if !self.latency_samples.is_empty() {
            let (avg, min, max) = self.calculate_latency_stats();
            println!("[INFO] Latency samples: {}", self.latency_samples.len());
            println!(
                "[INFO] Average latency: {:.3} ms ({:.2} us)",
                avg / 1_000_000.0,
                avg / 1_000.0
            );
            println!("[INFO] Min latency: {:.3} ms ({:.2} us)", min / 1_000_000.0, min / 1_000.0);
            println!("[INFO] Max latency: {:.3} ms ({:.2} us)", max / 1_000_000.0, max / 1_000.0);
        }
    }
}

pub fn get_current_time_ns() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64
}

pub const WARMUP_SEQ_NUM: u64 = u64::MAX;

pub fn create_test_data(size: usize, seq_num: u64) -> PerformanceTestData {
    PerformanceTestData { seq_num, timestamp: get_current_time_ns(), data: vec![0u8; size] }
}

pub fn create_warmup_test_data(size: usize) -> PerformanceTestData {
    PerformanceTestData {
        seq_num: WARMUP_SEQ_NUM,
        timestamp: get_current_time_ns(),
        data: vec![0u8; size],
    }
}

pub fn create_latency_data(size: usize, seq_num: u64) -> LatencyTestData {
    LatencyTestData {
        seq_num,
        send_timestamp: get_current_time_ns(),
        echo_timestamp: 0,
        data: vec![0u8; size],
    }
}

pub fn create_warmup_latency_data(size: usize) -> LatencyTestData {
    LatencyTestData {
        seq_num: WARMUP_SEQ_NUM,
        send_timestamp: get_current_time_ns(),
        echo_timestamp: 0,
        data: vec![0u8; size],
    }
}
