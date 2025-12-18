#[allow(dead_code)]
mod common;

use chrono::Local;
use clap::Parser;
use common::{
    create_latency_data, create_test_data, get_current_time_ns, LatencyTestData, PerformanceStats,
    PerformanceTestData, PublisherArgs, ReliabilityMode,
};
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{HistoryQosPolicy, HistoryQosPolicyKind, ResourceLimitsQosPolicy},
        status::StatusMask,
        wait_set::WaitSet,
    },
    publication::{
        data_writer_listener::DataWriterListener,
        qos::{DataWriterQos, PublisherQos},
    },
    subscription::{
        data_reader_listener::DataReaderListener,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::qos::TopicQos,
};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration as StdDuration, Instant},
};

struct ThroughputPubListener;

impl DataWriterListener for ThroughputPubListener {
    type Foo = PerformanceTestData;

    fn on_publication_matched(
        &self,
        _writer: &int2dds::publication::data_writer::DataWriter<Self::Foo>,
        status: &int2dds::infrastructure::status::PublicationMatchedStatus,
    ) {
        if status.current_count() > 0 {
            println!("Throughput test subscriber matched! Starting test...");
        } else {
            println!("No subscribers matched.");
        }
    }
}

struct LatencyPubListener;

impl DataWriterListener for LatencyPubListener {
    type Foo = LatencyTestData;

    fn on_publication_matched(
        &self,
        _writer: &int2dds::publication::data_writer::DataWriter<Self::Foo>,
        status: &int2dds::infrastructure::status::PublicationMatchedStatus,
    ) {
        if status.current_count() > 0 {
            println!("Latency test subscriber matched! Starting test...");
        } else {
            println!("No subscribers matched.");
        }
    }
}

struct LocalLatencyPubListener;

impl DataWriterListener for LocalLatencyPubListener {
    type Foo = PerformanceTestData;

    fn on_publication_matched(
        &self,
        _writer: &int2dds::publication::data_writer::DataWriter<Self::Foo>,
        status: &int2dds::infrastructure::status::PublicationMatchedStatus,
    ) {
        if status.current_count() > 0 {
            println!("Local latency test subscriber matched! Starting test...");
        } else {
            println!("No subscribers matched.");
        }
    }
}

struct LatencySubListener {
    writer: Arc<Mutex<Option<int2dds::publication::data_writer::DataWriter<LatencyTestData>>>>,
    stats: Arc<Mutex<PerformanceStats>>,
}

impl LatencySubListener {
    fn new(stats: Arc<Mutex<PerformanceStats>>) -> Self {
        Self { writer: Arc::new(Mutex::new(None)), stats }
    }

    fn set_writer(&self, writer: int2dds::publication::data_writer::DataWriter<LatencyTestData>) {
        *self.writer.lock().unwrap() = Some(writer);
    }
}

impl DataReaderListener for LatencySubListener {
    type Foo = LatencyTestData;

    fn on_data_available(
        &self,
        reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
    ) {
        let current_time = get_current_time_ns();

        if let Ok(samples) = reader.take(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            if let Some(writer) = self.writer.lock().unwrap().as_ref() {
                for sample in samples {
                    if let Ok(data) = sample.data() {
                        if data.echo_timestamp == 0 {
                            let echo_data = LatencyTestData {
                                seq_num: data.seq_num,
                                send_timestamp: data.send_timestamp,
                                echo_timestamp: current_time,
                                data: data.data.clone(),
                            };

                            if let Err(e) = writer.write(&echo_data, InstanceHandle::NIL) {
                                eprintln!("Failed to echo latency sample: {:?}", e);
                            }
                        } else {
                            let latency_ns = current_time - data.send_timestamp;
                            let mut stats = self.stats.lock().unwrap();
                            stats.add_latency_sample(latency_ns);
                            stats.add_sample(data.data.len());
                        }
                    }
                }
            }
        }
    }
}

fn run_throughput_test(args: &PublisherArgs) {
    let reliability = ReliabilityMode::from(args.common.reliability.as_str());

    println!("Starting throughput test:");
    println!("  Data size: {} bytes", args.common.data_len);
    println!("  Reliability: {}", reliability);
    println!("  Execution time: {} seconds", args.common.execution_time);
    println!("  Batch size: {}", args.batch_size);

    env_logger::builder().filter_level(log::LevelFilter::Error).init();

    let participant_qos = DomainParticipantQos::default();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(args.common.domain_id, participant_qos, None, StatusMask::default())
        .expect("Failed to create participant");

    let topic = participant
        .create_topic::<PerformanceTestData>(
            "throughput_test_topic",
            "ThroughputTestData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("Failed to create topic");

    let publisher = participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .expect("Failed to create publisher");

    let writer_qos = DataWriterQos {
        reliability: reliability.to_qos_policy(),
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(100) },
        resource_limits: ResourceLimitsQosPolicy {
            max_samples: 200,
            max_instances: 1,
            max_samples_per_instance: 200,
        },
        ..Default::default()
    };

    let listener = ThroughputPubListener;
    let writer = publisher
        .create_datawriter::<PerformanceTestData>(
            &topic,
            writer_qos,
            Some(Arc::new(listener)),
            StatusMask::default(),
        )
        .expect("Failed to create datawriter");

    println!("Waiting for subscriber to connect...");
    let condition = writer.get_statuscondition().expect("Failed to get status condition").clone();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).expect("Failed to attach condition");
    wait_set.wait(Duration::infinite()).expect("Failed to wait");

    println!("Subscriber connected. Starting throughput test...");

    // Warmup phase
    if args.common.warmup_time > 0 {
        println!("Warmup phase: sleeping for {} seconds...", args.common.warmup_time);
        thread::sleep(StdDuration::from_secs(args.common.warmup_time));
        println!("Warmup complete. Starting actual measurement...");
    }

    let start_time = Instant::now();
    let end_time = start_time + StdDuration::from_secs(args.common.execution_time);
    let mut seq_num = 0u64;
    let mut total_sent = 0u64;
    let mut last_report_time = start_time;
    let mut last_report_count = 0u64;

    // Hz-based timing
    let mut next_send_time = if args.common.hz.is_some() { Some(start_time) } else { None };

    while Instant::now() < end_time {
        let data = create_test_data(args.common.data_len, seq_num);

        let write_start = Instant::now();
        writer.write(&data, InstanceHandle::NIL).expect("Failed to write data");
        let write_duration = write_start.elapsed();

        if write_duration >= StdDuration::from_secs(1) {
            println!(
                "[WARNING] writer.write() took {:.2}s (seq_num: {}, total_sent: {})",
                write_duration.as_secs_f64(),
                seq_num,
                total_sent
            );
        }

        seq_num += 1;
        total_sent += 1;

        // Hz-based rate limiting
        if let Some(hz) = args.common.hz {
            if let Some(ref mut next_time) = next_send_time {
                let target_interval = StdDuration::from_secs_f64(1.0 / hz);
                *next_time += target_interval;

                let now = Instant::now();
                if *next_time > now {
                    thread::sleep(*next_time - now);
                }
            }
        }

        let now = Instant::now();
        if now.duration_since(last_report_time) >= StdDuration::from_secs(5) {
            let elapsed = now.duration_since(start_time).as_secs_f64();
            let interval_elapsed = now.duration_since(last_report_time).as_secs_f64();
            let interval_sent = total_sent - last_report_count;

            let avg_rate = total_sent as f64 / elapsed;
            let interval_rate = interval_sent as f64 / interval_elapsed;
            let avg_mbps =
                (total_sent as f64 * args.common.data_len as f64 * 8.0) / (elapsed * 1_000_000.0);
            let interval_mbps = (interval_sent as f64 * args.common.data_len as f64 * 8.0)
                / (interval_elapsed * 1_000_000.0);

            println!(
                "[{:.1}s] Sent: {}, Avg: {:.0} msg/s ({:.2} Mbps), Interval: {:.0} msg/s ({:.2} Mbps)",
                elapsed, total_sent, avg_rate, avg_mbps, interval_rate, interval_mbps
            );

            last_report_time = now;
            last_report_count = total_sent;
        }
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    let final_rate = total_sent as f64 / elapsed;
    let mbps = (total_sent as f64 * args.common.data_len as f64 * 8.0) / (elapsed * 1_000_000.0);

    println!("\n=== Throughput Test Publisher Results ===");
    println!("[INFO] Total samples sent: {}", total_sent);
    println!("Test duration: {:.2} seconds", elapsed);
    println!("[INFO] Send rate: {:.2} msg/s", final_rate);
    println!("Throughput: {:.2} Mbps", mbps);

    let log_content = format!(
        "Total sent: {}\nDuration: {:.2}s\nRate: {:.2} msg/s\nThroughput: {:.2} Mbps\n",
        total_sent, elapsed, final_rate, mbps
    );
    println!("\n{}", log_content);
    let timestamp = Local::now().format("%Y%m%dT%H%M%S");
    let filename = format!("pub_thr-{}.log", timestamp);
    std::fs::write(&filename, log_content).expect("Failed to write log file");

    // CSV output
    let csv_filename = if let Some(hz) = args.common.hz {
        format!("publisher_throughput_results_{}b_{}hz.csv", args.common.data_len, hz)
    } else {
        format!("publisher_throughput_results_{}b.csv", args.common.data_len)
    };
    let file_exists = std::path::Path::new(&csv_filename).exists();
    let mut csv_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&csv_filename)
        .expect("Failed to open CSV file");

    use std::io::Write;

    if !file_exists {
        writeln!(
            csv_file,
            "Timestamp,Total_Sent_Messages,Duration_Seconds,Messages_Per_Second,Throughput_Mbps"
        )
        .expect("Failed to write CSV header");
    }

    writeln!(
        csv_file,
        "{},{},{:.2},{:.2},{:.2}",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        total_sent,
        elapsed,
        final_rate,
        mbps
    )
    .expect("Failed to write CSV data");
}

fn run_latency_test(args: &PublisherArgs) {
    let reliability = ReliabilityMode::from(args.common.reliability.as_str());

    println!("Starting latency test:");
    println!("  Data size: {} bytes", args.common.data_len);
    println!("  Reliability: {}", reliability);
    if let Some(count) = args.latency_count {
        println!("  Latency count: {} samples", count);
    } else {
        println!("  Execution time: {} seconds", args.common.execution_time);
    }
    if let Some(hz) = args.common.hz {
        println!("  Rate: {} Hz ({:.3} ms interval)", hz, 1000.0 / hz);
    } else {
        println!("  Rate: unlimited");
    }

    env_logger::builder().filter_level(log::LevelFilter::Error).init();

    let participant_qos = DomainParticipantQos::default();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(args.common.domain_id, participant_qos, None, StatusMask::default())
        .expect("Failed to create participant");

    let topic = participant
        .create_topic::<LatencyTestData>(
            "latency_test_topic",
            "LatencyTestData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("Failed to create topic");
    let topic_echo = participant
        .create_topic::<LatencyTestData>(
            "latency_test_topic_echo",
            "LatencyTestData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("Failed to create topic");

    let publisher = participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .expect("Failed to create publisher");
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .expect("Failed to create subscriber");

    let writer_qos = DataWriterQos {
        reliability: reliability.to_qos_policy(),
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(100) },
        resource_limits: ResourceLimitsQosPolicy {
            max_samples: 200,
            max_instances: 1,
            max_samples_per_instance: 200,
        },
        ..Default::default()
    };

    let reader_qos =
        DataReaderQos { reliability: reliability.to_qos_policy(), ..Default::default() };

    let stats = Arc::new(Mutex::new(PerformanceStats::new()));
    let listener = Arc::new(LatencySubListener::new(Arc::clone(&stats)));

    let pub_listener = LatencyPubListener;
    let writer = publisher
        .create_datawriter::<LatencyTestData>(
            &topic,
            writer_qos,
            Some(Arc::new(pub_listener)),
            StatusMask::default(),
        )
        .expect("Failed to create datawriter");

    let _reader = subscriber
        .create_datareader::<LatencyTestData>(
            &topic_echo,
            reader_qos,
            Some(Arc::clone(&listener) as Arc<dyn DataReaderListener<Foo = LatencyTestData>>),
            StatusMask::default(),
        )
        .expect("Failed to create datareader");

    listener.set_writer(writer.clone());

    println!("Waiting for subscriber to connect...");
    let condition = writer.get_statuscondition().expect("Failed to get status condition").clone();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).expect("Failed to attach condition");
    wait_set.wait(Duration::infinite()).expect("Failed to wait");

    println!("Subscriber connected. Starting latency test...");

    // Warmup phase
    if args.common.warmup_time > 0 {
        println!("Warmup phase: sleeping for {} seconds...", args.common.warmup_time);
        thread::sleep(StdDuration::from_secs(args.common.warmup_time));
        println!("Warmup complete. Starting actual measurement...");
    }

    let start_time = Instant::now();
    let end_time = start_time + StdDuration::from_secs(args.common.execution_time);
    let mut seq_num = 0u64;

    stats.lock().unwrap().start_time = start_time;

    // Hz-based timing
    let mut next_send_time = if args.common.hz.is_some() { Some(start_time) } else { None };

    if let Some(max_count) = args.latency_count {
        // Count-based mode: send up to N samples, but stop if execution_time expires
        println!(
            "Sending {} latency samples (max {} seconds)...",
            max_count, args.common.execution_time
        );
        while seq_num < max_count && Instant::now() < end_time {
            let data = create_latency_data(args.common.data_len, seq_num);
            writer.write(&data, InstanceHandle::NIL).expect("Failed to write data");
            seq_num += 1;

            if seq_num % 100 == 0 {
                println!("Sent: {} / {}", seq_num, max_count);
            }

            // Hz-based rate limiting
            if let Some(hz) = args.common.hz {
                if let Some(ref mut next_time) = next_send_time {
                    let target_interval = StdDuration::from_secs_f64(1.0 / hz);
                    *next_time += target_interval;

                    let now = Instant::now();
                    if *next_time > now {
                        thread::sleep(*next_time - now);
                    }
                }
            }
        }

        if seq_num < max_count {
            println!(
                "Test stopped by execution_time limit. Sent {} / {} samples.",
                seq_num, max_count
            );
        }
    } else {
        // Time-based mode: send samples for execution_time duration
        while Instant::now() < end_time {
            let data = create_latency_data(args.common.data_len, seq_num);
            writer.write(&data, InstanceHandle::NIL).expect("Failed to write data");
            seq_num += 1;

            // Hz-based rate limiting
            if let Some(hz) = args.common.hz {
                if let Some(ref mut next_time) = next_send_time {
                    let target_interval = StdDuration::from_secs_f64(1.0 / hz);
                    *next_time += target_interval;

                    let now = Instant::now();
                    if *next_time > now {
                        thread::sleep(*next_time - now);
                    }
                }
            }
        }
    }

    thread::sleep(StdDuration::from_millis(1000));

    let mut final_stats = stats.lock().unwrap();
    final_stats.end_time = Instant::now();
    final_stats.print_summary(false);

    let (avg, min, max) = final_stats.calculate_latency_stats();
    let log_content = format!(
        "Latency samples: {}\nAvg latency: {:.3} ms ({:.2} us)\nMin latency: {:.3} ms ({:.2} us)\nMax latency: {:.3} ms ({:.2} us)\n",
        final_stats.latency_samples.len(), avg / 1_000_000.0, avg / 1_000.0, min / 1_000_000.0, min / 1_000.0, max / 1_000_000.0, max / 1_000.0
    );
    let timestamp = Local::now().format("%Y%m%dT%H%M%S");
    let filename = format!("pub_lat-{}.log", timestamp);
    std::fs::write(&filename, log_content).expect("Failed to write log file");

    // CSV output
    let hz_str = args.common.hz.map_or("unlimited".to_string(), |h| format!("{}hz", h as u64));
    let csv_filename =
        format!("publisher_latency_results_{}b_{}.csv", args.common.data_len, hz_str);
    let file_exists = std::path::Path::new(&csv_filename).exists();
    let mut csv_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&csv_filename)
        .expect("Failed to open CSV file");

    use std::io::Write;

    if !file_exists {
        writeln!(csv_file, "Timestamp,Average_Latency_ms,Min_Latency_ms,Max_Latency_ms")
            .expect("Failed to write CSV header");
    }

    writeln!(
        csv_file,
        "{},{:.3},{:.3},{:.3}",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        avg / 1_000_000.0,
        min / 1_000_000.0,
        max / 1_000_000.0
    )
    .expect("Failed to write CSV data");
}

fn run_local_latency_test(args: &PublisherArgs) {
    let reliability = ReliabilityMode::from(args.common.reliability.as_str());

    println!("Starting local latency test (publisher):");
    println!("  Data size: {} bytes", args.common.data_len);
    println!("  Reliability: {}", reliability);
    println!("  Execution time: {} seconds", args.common.execution_time);
    if let Some(hz) = args.common.hz {
        println!("  Rate: {} Hz ({:.3} ms interval)", hz, 1000.0 / hz);
    } else {
        println!("  Rate: unlimited");
    }

    env_logger::builder().filter_level(log::LevelFilter::Error).init();

    let participant_qos = DomainParticipantQos::default();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(args.common.domain_id, participant_qos, None, StatusMask::default())
        .expect("Failed to create participant");

    // Use same topic as throughput test
    let topic = participant
        .create_topic::<PerformanceTestData>(
            "local_latency_test_topic",
            "ThroughputTestData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("Failed to create topic");

    let publisher = participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .expect("Failed to create publisher");

    let writer_qos = DataWriterQos {
        reliability: reliability.to_qos_policy(),
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(100) },
        resource_limits: ResourceLimitsQosPolicy {
            max_samples: 200,
            max_instances: 1,
            max_samples_per_instance: 200,
        },
        ..Default::default()
    };

    let listener = LocalLatencyPubListener;
    let writer = publisher
        .create_datawriter::<PerformanceTestData>(
            &topic,
            writer_qos,
            Some(Arc::new(listener)),
            StatusMask::default(),
        )
        .expect("Failed to create datawriter");

    println!("Waiting for subscriber to connect...");
    let condition = writer.get_statuscondition().expect("Failed to get status condition").clone();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).expect("Failed to attach condition");
    wait_set.wait(Duration::infinite()).expect("Failed to wait");

    println!("Subscriber connected. Starting local latency test...");

    // Warmup phase
    if args.common.warmup_time > 0 {
        println!("Warmup phase: sleeping for {} seconds...", args.common.warmup_time);
        thread::sleep(StdDuration::from_secs(args.common.warmup_time));
        println!("Warmup complete. Starting actual measurement...");
    }

    let start_time = Instant::now();
    let end_time = start_time + StdDuration::from_secs(args.common.execution_time);
    let mut seq_num = 0u64;
    let mut total_sent = 0u64;
    let mut last_report_time = start_time;
    let mut last_report_count = 0u64;

    // Hz-based timing
    let mut next_send_time = if args.common.hz.is_some() { Some(start_time) } else { None };

    while Instant::now() < end_time {
        let data = create_test_data(args.common.data_len, seq_num);

        writer.write(&data, InstanceHandle::NIL).expect("Failed to write data");

        seq_num += 1;
        total_sent += 1;

        // Hz-based rate limiting
        if let Some(hz) = args.common.hz {
            if let Some(ref mut next_time) = next_send_time {
                let target_interval = StdDuration::from_secs_f64(1.0 / hz);
                *next_time += target_interval;

                let now = Instant::now();
                if *next_time > now {
                    thread::sleep(*next_time - now);
                }
            }
        }

        let now = Instant::now();
        if now.duration_since(last_report_time) >= StdDuration::from_secs(5) {
            let elapsed = now.duration_since(start_time).as_secs_f64();
            let interval_elapsed = now.duration_since(last_report_time).as_secs_f64();
            let interval_sent = total_sent - last_report_count;

            let avg_rate = total_sent as f64 / elapsed;
            let interval_rate = interval_sent as f64 / interval_elapsed;

            println!(
                "[{:.1}s] Sent: {}, Avg: {:.0} msg/s, Interval: {:.0} msg/s",
                elapsed, total_sent, avg_rate, interval_rate
            );

            last_report_time = now;
            last_report_count = total_sent;
        }
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    let final_rate = total_sent as f64 / elapsed;

    println!("\n=== Local Latency Test Publisher Results ===");
    println!("[INFO] Total samples sent: {}", total_sent);
    println!("Test duration: {:.2} seconds", elapsed);
    println!("[INFO] Send rate: {:.2} msg/s", final_rate);

    let log_content = format!(
        "Local Latency Test Publisher\nTotal sent: {}\nDuration: {:.2}s\nRate: {:.2} msg/s\n",
        total_sent, elapsed, final_rate
    );
    let timestamp = Local::now().format("%Y%m%dT%H%M%S");
    let filename = format!("pub_local_lat-{}.log", timestamp);
    std::fs::write(&filename, log_content).expect("Failed to write log file");

    // CSV output
    let csv_filename = if let Some(hz) = args.common.hz {
        format!("publisher_local_latency_results_{}b_{}hz.csv", args.common.data_len, hz)
    } else {
        format!("publisher_local_latency_results_{}b.csv", args.common.data_len)
    };
    let file_exists = std::path::Path::new(&csv_filename).exists();
    let mut csv_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&csv_filename)
        .expect("Failed to open CSV file");

    use std::io::Write;

    if !file_exists {
        writeln!(csv_file, "Timestamp,Total_Sent_Messages,Duration_Seconds,Messages_Per_Second")
            .expect("Failed to write CSV header");
    }

    writeln!(
        csv_file,
        "{},{},{:.2},{:.2}",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        total_sent,
        elapsed,
        final_rate
    )
    .expect("Failed to write CSV data");
}

fn main() {
    let args = PublisherArgs::parse();

    println!("DDS Performance Test Publisher");
    println!("Test mode: {}", args.common.test_mode);

    match args.common.test_mode.to_lowercase().as_str() {
        "throughput" | "thr" | "1" => {
            println!("Starting throughput test mode");
            run_throughput_test(&args)
        }
        "latency" | "lat" | "2" => {
            println!("Starting latency test mode");
            run_latency_test(&args)
        }
        "local_latency" | "local" | "ll" | "3" => {
            println!("Starting local latency test mode");
            run_local_latency_test(&args)
        }
        _ => {
            println!(
                "Invalid test mode '{}'. Supported modes: throughput, latency, local_latency",
                args.common.test_mode
            );
            println!("Defaulting to throughput test.");
            run_throughput_test(&args)
        }
    }
}
