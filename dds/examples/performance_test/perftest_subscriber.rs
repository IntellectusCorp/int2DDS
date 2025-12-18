#[allow(dead_code)]
mod common;

use chrono::Local;
use clap::Parser;
use common::{
    get_current_time_ns, LatencyTestData, PerformanceStats, PerformanceTestData, ReliabilityMode,
    SubscriberArgs,
};
use int2dds::{
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::status::StatusMask,
    subscription::{
        data_reader_listener::DataReaderListener,
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    topic::qos::TopicQos,
};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration as StdDuration, Instant},
};
#[allow(dead_code)]
struct ThroughputSubListener {
    stats: Arc<Mutex<PerformanceStats>>,
    expected_seq: Arc<Mutex<u64>>,
    out_of_order: bool,
    execution_time: u64,
    last_message_time_ns: Arc<AtomicU64>,
    start_instant: Instant,
    should_stop: Arc<AtomicBool>,
    first_sample_received: Arc<AtomicBool>,
}

impl ThroughputSubListener {
    fn new(out_of_order: bool, execution_time: u64) -> Self {
        Self {
            stats: Arc::new(Mutex::new(PerformanceStats::new())),
            expected_seq: Arc::new(Mutex::new(0)),
            out_of_order,
            execution_time,
            last_message_time_ns: Arc::new(AtomicU64::new(0)),
            start_instant: Instant::now(),
            should_stop: Arc::new(AtomicBool::new(false)),
            first_sample_received: Arc::new(AtomicBool::new(false)),
        }
    }

    fn get_stats(&self) -> Arc<Mutex<PerformanceStats>> {
        Arc::clone(&self.stats)
    }

    fn get_last_message_time_ns(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.last_message_time_ns)
    }

    fn get_start_instant(&self) -> Instant {
        self.start_instant
    }

    fn get_should_stop(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.should_stop)
    }
}

impl DataReaderListener for ThroughputSubListener {
    type Foo = PerformanceTestData;

    fn on_subscription_matched(
        &self,
        _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        status: &int2dds::infrastructure::status::SubscriptionMatchedStatus,
    ) {
        if status.current_count() > 0 {
            println!("Publisher matched! Waiting for data...");
        }
    }

    fn on_data_available(
        &self,
        reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
    ) {
        // Check if we should stop processing (lock-free)
        if self.should_stop.load(Ordering::Relaxed) {
            return;
        }

        if let Ok(samples) = reader.take(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            let now = Instant::now();

            // Update last message time (lock-free atomic operation)
            let elapsed_ns = now.duration_since(self.start_instant).as_nanos() as u64;
            self.last_message_time_ns.store(elapsed_ns, Ordering::Release);

            // Process samples with single stats lock
            let mut stats = self.stats.lock().unwrap();

            // Set start_time on first sample (after warmup) - lock-free atomic check
            if self
                .first_sample_received
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                stats.start_time = now;
                println!("First data sample received. Starting measurements...");
            }

            if self.out_of_order {
                // Out-of-order mode: count all samples regardless of seq_num, skip duplicates
                for sample in samples {
                    if let Ok(data) = sample.data() {
                        if stats.seen_seqs.insert(data.seq_num) {
                            stats.add_sample(data.data.len());
                        }
                    }
                }
            } else {
                // In-order mode: track expected_seq and detect lost samples
                let mut expected_seq = self.expected_seq.lock().unwrap();

                for sample in samples {
                    if let Ok(data) = sample.data() {
                        stats.seen_seqs.insert(data.seq_num);
                        if data.seq_num >= *expected_seq {
                            // Lost samples = gap between expected and received
                            stats.lost_samples += data.seq_num - *expected_seq;
                            *expected_seq = data.seq_num + 1;

                            // Only count this sample (not duplicates or out-of-order)
                            stats.add_sample(data.data.len());
                        }
                        // Skip samples with seq_num < expected_seq (duplicates or out-of-order)
                    }
                }
            }
        }
    }
}

struct LatencySubListener {
    writer: Arc<Mutex<Option<int2dds::publication::data_writer::DataWriter<LatencyTestData>>>>,
    stats: Arc<Mutex<PerformanceStats>>,
    last_report_time_ns: Arc<AtomicU64>,
    start_instant: Instant,
    first_sample_received: Arc<AtomicBool>,
}

#[allow(dead_code)]
struct LocalLatencySubListener {
    stats: Arc<Mutex<PerformanceStats>>,
    last_message_time_ns: Arc<AtomicU64>,
    start_instant: Instant,
    should_stop: Arc<AtomicBool>,
    first_sample_received: Arc<AtomicBool>,
}

impl LocalLatencySubListener {
    fn new() -> Self {
        Self {
            stats: Arc::new(Mutex::new(PerformanceStats::new())),
            last_message_time_ns: Arc::new(AtomicU64::new(0)),
            start_instant: Instant::now(),
            should_stop: Arc::new(AtomicBool::new(false)),
            first_sample_received: Arc::new(AtomicBool::new(false)),
        }
    }

    fn get_stats(&self) -> Arc<Mutex<PerformanceStats>> {
        Arc::clone(&self.stats)
    }

    fn get_last_message_time_ns(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.last_message_time_ns)
    }

    fn get_start_instant(&self) -> Instant {
        self.start_instant
    }

    fn get_should_stop(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.should_stop)
    }
}

impl DataReaderListener for LocalLatencySubListener {
    type Foo = PerformanceTestData;

    fn on_subscription_matched(
        &self,
        _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        status: &int2dds::infrastructure::status::SubscriptionMatchedStatus,
    ) {
        if status.current_count() > 0 {
            println!("Local latency test publisher matched! Waiting for data...");
        }
    }

    fn on_data_available(
        &self,
        reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
    ) {
        let receive_time_ns = get_current_time_ns();
        let now = Instant::now();

        if self.should_stop.load(Ordering::Relaxed) {
            return;
        }

        if let Ok(samples) = reader.take(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            // Update last message time
            let elapsed_ns = now.duration_since(self.start_instant).as_nanos() as u64;
            self.last_message_time_ns.store(elapsed_ns, Ordering::Release);

            let mut stats = self.stats.lock().unwrap();

            // Set start_time on first sample
            if self
                .first_sample_received
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                stats.start_time = now;
                println!("First local latency sample received. Starting measurements...");
            }

            for sample in samples {
                if let Ok(data) = sample.data() {
                    // Skip warmup samples
                    if data.seq_num == common::WARMUP_SEQ_NUM {
                        continue;
                    }

                    stats.add_sample(data.data.len());
                    stats.seen_seqs.insert(data.seq_num);

                    // Calculate one-way latency: receive_time - send_timestamp
                    let latency_ns = receive_time_ns.saturating_sub(data.timestamp);
                    stats.latency_samples.push(latency_ns as f64);
                }
            }
        }
    }
}

impl LatencySubListener {
    fn new() -> Self {
        Self {
            writer: Arc::new(Mutex::new(None)),
            stats: Arc::new(Mutex::new(PerformanceStats::new())),
            last_report_time_ns: Arc::new(AtomicU64::new(0)),
            start_instant: Instant::now(),
            first_sample_received: Arc::new(AtomicBool::new(false)),
        }
    }

    fn set_writer(&self, writer: int2dds::publication::data_writer::DataWriter<LatencyTestData>) {
        *self.writer.lock().unwrap() = Some(writer);
    }

    fn get_stats(&self) -> Arc<Mutex<PerformanceStats>> {
        Arc::clone(&self.stats)
    }
}

impl DataReaderListener for LatencySubListener {
    type Foo = LatencyTestData;

    fn on_subscription_matched(
        &self,
        _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        status: &int2dds::infrastructure::status::SubscriptionMatchedStatus,
    ) {
        if status.current_count() > 0 {
            println!("Latency test publisher matched! Waiting for data...");
        }
    }

    fn on_data_available(
        &self,
        reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
    ) {
        let current_time = get_current_time_ns();
        let now = Instant::now();

        if let Ok(samples) = reader.take(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            // Set start_time on first sample (after warmup) - lock-free atomic operation
            if self
                .first_sample_received
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let mut stats = self.stats.lock().unwrap();
                stats.start_time = now;
                drop(stats);
                println!("First latency sample received. Starting measurements...");
            }

            if let Some(writer) = self.writer.lock().unwrap().as_ref() {
                for sample in &samples {
                    if let Ok(data) = sample.data() {
                        // Echo back to publisher
                        if data.echo_timestamp == 0 {
                            let echo_data = LatencyTestData {
                                seq_num: data.seq_num,
                                send_timestamp: data.send_timestamp,
                                echo_timestamp: current_time,
                                data: data.data.clone(),
                            };

                            if let Err(e) = writer.write(
                                &echo_data,
                                int2dds::common::instance_handle::InstanceHandle::NIL,
                            ) {
                                eprintln!("Failed to echo latency sample: {:?}", e);
                            }
                        }
                    }
                }
            }

            let mut stats = self.stats.lock().unwrap();
            for sample in samples {
                if let Ok(data) = sample.data() {
                    stats.add_sample(data.data.len());
                }
            }
            drop(stats);

            // Check if 5 seconds passed since last report (lock-free atomic operation)
            let now_ns = now.duration_since(self.start_instant).as_nanos() as u64;
            let last_report_ns = self.last_report_time_ns.load(Ordering::Relaxed);

            if now_ns - last_report_ns >= 5_000_000_000 {
                // 5 seconds in nanoseconds
                // Try to update the last report time atomically
                if self
                    .last_report_time_ns
                    .compare_exchange(last_report_ns, now_ns, Ordering::Release, Ordering::Relaxed)
                    .is_ok()
                {
                    println!("Echo mode: responding to latency pings");
                }
            }
        }
    }
}

fn run_throughput_subscriber(args: &SubscriberArgs) {
    let reliability = ReliabilityMode::from(args.common.reliability.as_str());

    println!("Starting throughput subscriber:");
    println!("  Reliability: {}", reliability);
    println!("  Execution time: {} seconds", args.common.execution_time);
    println!("  Out-of-order mode: {}", args.out_of_order);

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

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .expect("Failed to create subscriber");

    let reader_qos =
        DataReaderQos { reliability: reliability.to_qos_policy(), ..Default::default() };

    let listener =
        Arc::new(ThroughputSubListener::new(args.out_of_order, args.common.execution_time));
    let stats = listener.get_stats();
    let last_message_time_ns = listener.get_last_message_time_ns();
    let start_instant = listener.get_start_instant();
    let should_stop = listener.get_should_stop();

    let _reader = subscriber
        .create_datareader::<PerformanceTestData>(
            &topic,
            reader_qos,
            Some(Arc::clone(&listener) as Arc<dyn DataReaderListener<Foo = PerformanceTestData>>),
            StatusMask::default(),
        )
        .expect("Failed to create datareader");

    println!("Waiting for publisher...");

    // Spawn timeout monitoring thread
    let last_msg_ns_clone = Arc::clone(&last_message_time_ns);
    let should_stop_clone = Arc::clone(&should_stop);
    let timeout_thread = thread::spawn(move || {
        // Wait for first message
        loop {
            if last_msg_ns_clone.load(Ordering::Acquire) > 0 {
                break;
            }
            thread::sleep(StdDuration::from_millis(100));
        }

        // Monitor for 2 second timeout after last message
        loop {
            thread::sleep(StdDuration::from_millis(100));

            let last_ns = last_msg_ns_clone.load(Ordering::Acquire);
            if last_ns > 0 {
                let last_instant = start_instant + StdDuration::from_nanos(last_ns);
                if last_instant.elapsed() >= StdDuration::from_secs(2) {
                    println!("\n2 second timeout after last message. Stopping...");
                    should_stop_clone.store(true, Ordering::Release);
                    break;
                }
            }
        }
    });

    // Spawn a thread for periodic reporting
    let stats_clone = Arc::clone(&stats);
    let execution_time = args.common.execution_time;
    let out_of_order = args.out_of_order;
    let should_stop_clone2 = Arc::clone(&should_stop);
    let report_thread = thread::spawn(move || {
        // Wait for first sample (measurement started)
        let measurement_start;
        loop {
            let stats = stats_clone.lock().unwrap();
            if stats.total_samples > 0 {
                measurement_start = stats.start_time;
                drop(stats);
                break;
            }
            drop(stats);
            thread::sleep(StdDuration::from_millis(100));
        }

        // Report every 5 seconds from measurement start
        for interval in (5..=execution_time).step_by(5) {
            // Wait until this interval from measurement start or should_stop
            loop {
                // Check if should stop (lock-free)
                if should_stop_clone2.load(Ordering::Acquire) {
                    return;
                }

                let elapsed = measurement_start.elapsed();
                if elapsed >= StdDuration::from_secs(interval) {
                    break;
                }
                thread::sleep(StdDuration::from_millis(100));
            }

            // Check again before reporting (lock-free)
            if should_stop_clone2.load(Ordering::Acquire) {
                return;
            }

            let mut stats_mut = stats_clone.lock().unwrap();
            stats_mut.end_time = measurement_start + StdDuration::from_secs(interval);

            if out_of_order {
                stats_mut.calculate_lost_samples_from_hashset();
            }

            let (msgs_per_sec, mbps) = stats_mut.calculate_throughput();
            let max_seq = stats_mut.seen_seqs.iter().max().copied().unwrap_or(0);
            let loss_rate = if max_seq > 0 {
                (stats_mut.lost_samples as f64 / max_seq as f64) * 100.0
            } else {
                0.0
            };
            println!(
                "[{}s] Received: {}, Rate: {:.0} msg/s, {:.2} Mbps, Lost: {}, Last seq: {}, Loss rate: {:.2}%",
                interval, stats_mut.total_samples, msgs_per_sec, mbps, stats_mut.lost_samples, max_seq, loss_rate
            );
        }
    });

    // Wait for timeout thread or execution time, whichever comes first
    loop {
        if should_stop.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(StdDuration::from_millis(100));
    }

    // Wait for threads to finish
    let _ = timeout_thread.join();
    let _ = report_thread.join();

    let mut final_stats = stats.lock().unwrap();
    // Set end_time to actual last message time for accurate measurement
    let last_ns = last_message_time_ns.load(Ordering::Acquire);
    if last_ns > 0 {
        final_stats.end_time = start_instant + StdDuration::from_nanos(last_ns);
    } else {
        // Fallback if no messages received
        final_stats.end_time = final_stats.start_time;
    }
    final_stats.print_summary(args.out_of_order);

    let (msgs_per_sec, mbps) = final_stats.calculate_throughput();
    let log_content = format!(
        "Total received: {}\nLost samples: {}\nRate: {:.2} msg/s\nThroughput: {:.2} Mbps\n",
        final_stats.total_samples, final_stats.lost_samples, msgs_per_sec, mbps
    );
    let timestamp = Local::now().format("%Y%m%dT%H%M%S");
    let filename = format!("sub_thr-{}.log", timestamp);
    std::fs::write(&filename, log_content).expect("Failed to write log file");

    // CSV output
    let csv_filename = if let Some(hz) = args.common.hz {
        format!("subscriber_throughput_results_{}b_{}hz.csv", args.common.data_len, hz)
    } else {
        format!("subscriber_throughput_results_{}b.csv", args.common.data_len)
    };
    let file_exists = std::path::Path::new(&csv_filename).exists();
    let mut csv_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&csv_filename)
        .expect("Failed to open CSV file");

    use std::io::Write;

    if !file_exists {
        writeln!(csv_file, "Timestamp,Total_Received_Messages,Messages_Per_Second,Lost_Messages,Loss_Rate_Percent,Throughput_Mbps")
            .expect("Failed to write CSV header");
    }

    let max_seq = final_stats.seen_seqs.iter().max().copied().unwrap_or(0);
    let loss_rate =
        if max_seq > 0 { (final_stats.lost_samples as f64 / max_seq as f64) * 100.0 } else { 0.0 };

    writeln!(
        csv_file,
        "{},{},{:.2},{},{:.2},{:.2}",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        final_stats.total_samples,
        msgs_per_sec,
        final_stats.lost_samples,
        loss_rate,
        mbps
    )
    .expect("Failed to write CSV data");
}

fn run_latency_subscriber(args: &SubscriberArgs) {
    let reliability = ReliabilityMode::from(args.common.reliability.as_str());

    println!("Starting latency subscriber (echo mode):");
    println!("  Reliability: {}", reliability);
    println!("  Execution time: {} seconds (from first sample)", args.common.execution_time);

    env_logger::builder().filter_level(log::LevelFilter::Warn).init();

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

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .expect("Failed to create subscriber");

    let publisher = participant
        .create_publisher(
            int2dds::publication::qos::PublisherQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("Failed to create publisher");

    let reader_qos =
        DataReaderQos { reliability: reliability.to_qos_policy(), ..Default::default() };

    let writer_qos = int2dds::publication::qos::DataWriterQos {
        reliability: reliability.to_qos_policy(),
        ..Default::default()
    };

    let listener = Arc::new(LatencySubListener::new());
    let stats = listener.get_stats();

    let writer = publisher
        .create_datawriter::<LatencyTestData>(&topic_echo, writer_qos, None, StatusMask::default())
        .expect("Failed to create datawriter");

    listener.set_writer(writer);

    let _reader = subscriber
        .create_datareader::<LatencyTestData>(
            &topic,
            reader_qos,
            Some(Arc::clone(&listener) as Arc<dyn DataReaderListener<Foo = LatencyTestData>>),
            StatusMask::default(),
        )
        .expect("Failed to create datareader");

    println!("Waiting for latency test publisher...");
    println!("Ready to echo latency measurements back to publisher");

    // Wait for first sample
    let measurement_start;
    loop {
        let stats_guard = stats.lock().unwrap();
        if stats_guard.total_samples > 0 {
            measurement_start = stats_guard.start_time;
            println!(
                "First latency sample received. Starting {} second timer.",
                args.common.execution_time
            );
            drop(stats_guard);
            break;
        }
        drop(stats_guard);
        thread::sleep(StdDuration::from_millis(100));
    }

    // Wait for execution_time from first sample
    let target_time = measurement_start + StdDuration::from_secs(args.common.execution_time);
    loop {
        let now = Instant::now();
        if now >= target_time {
            break;
        }
        let remaining = target_time - now;
        if remaining > StdDuration::from_secs(1) {
            thread::sleep(StdDuration::from_secs(1));
        } else {
            thread::sleep(remaining);
            break;
        }
    }

    let final_stats = stats.lock().unwrap();
    println!("\n=== Latency Subscriber (Echo Mode) Results ===");
    println!("[INFO] Total echo responses sent: {}", final_stats.total_samples);

    let log_content = format!("Echo mode - Total responses: {}\n", final_stats.total_samples);
    let timestamp = Local::now().format("%Y%m%dT%H%M%S");
    let filename = format!("sub_lat-{}.log", timestamp);
    std::fs::write(&filename, log_content).expect("Failed to write log file");
}

fn run_local_latency_subscriber(args: &SubscriberArgs) {
    let reliability = ReliabilityMode::from(args.common.reliability.as_str());

    println!("Starting local latency subscriber:");
    println!("  Reliability: {}", reliability);
    println!("  Execution time: {} seconds", args.common.execution_time);

    env_logger::builder().filter_level(log::LevelFilter::Error).init();

    let participant_qos = DomainParticipantQos::default();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(args.common.domain_id, participant_qos, None, StatusMask::default())
        .expect("Failed to create participant");

    // Use throughput topic (same as throughput test publisher)
    let topic = participant
        .create_topic::<PerformanceTestData>(
            "local_latency_test_topic",
            "ThroughputTestData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .expect("Failed to create topic");

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .expect("Failed to create subscriber");

    let reader_qos =
        DataReaderQos { reliability: reliability.to_qos_policy(), ..Default::default() };

    let listener = Arc::new(LocalLatencySubListener::new());
    let stats = listener.get_stats();
    let last_message_time_ns = listener.get_last_message_time_ns();
    let start_instant = listener.get_start_instant();
    let should_stop = listener.get_should_stop();

    let _reader = subscriber
        .create_datareader::<PerformanceTestData>(
            &topic,
            reader_qos,
            Some(Arc::clone(&listener) as Arc<dyn DataReaderListener<Foo = PerformanceTestData>>),
            StatusMask::default(),
        )
        .expect("Failed to create datareader");

    println!("Waiting for publisher...");

    // Spawn timeout monitoring thread
    let last_msg_ns_clone = Arc::clone(&last_message_time_ns);
    let should_stop_clone = Arc::clone(&should_stop);
    let timeout_thread = thread::spawn(move || {
        // Wait for first message
        loop {
            if last_msg_ns_clone.load(Ordering::Acquire) > 0 {
                break;
            }
            thread::sleep(StdDuration::from_millis(100));
        }

        // Monitor for 2 second timeout after last message
        loop {
            thread::sleep(StdDuration::from_millis(100));

            let last_ns = last_msg_ns_clone.load(Ordering::Acquire);
            if last_ns > 0 {
                let last_instant = start_instant + StdDuration::from_nanos(last_ns);
                if last_instant.elapsed() >= StdDuration::from_secs(2) {
                    println!("\n2 second timeout after last message. Stopping...");
                    should_stop_clone.store(true, Ordering::Release);
                    break;
                }
            }
        }
    });

    // Spawn a thread for periodic reporting
    let stats_clone = Arc::clone(&stats);
    let execution_time = args.common.execution_time;
    let should_stop_clone2 = Arc::clone(&should_stop);
    let report_thread = thread::spawn(move || {
        // Wait for first sample
        let measurement_start;
        loop {
            let stats = stats_clone.lock().unwrap();
            if stats.total_samples > 0 {
                measurement_start = stats.start_time;
                drop(stats);
                break;
            }
            drop(stats);
            thread::sleep(StdDuration::from_millis(100));
        }

        // Report every 5 seconds
        for interval in (5..=execution_time).step_by(5) {
            loop {
                if should_stop_clone2.load(Ordering::Acquire) {
                    return;
                }

                let elapsed = measurement_start.elapsed();
                if elapsed >= StdDuration::from_secs(interval) {
                    break;
                }
                thread::sleep(StdDuration::from_millis(100));
            }

            if should_stop_clone2.load(Ordering::Acquire) {
                return;
            }

            let stats = stats_clone.lock().unwrap();
            let sample_count = stats.latency_samples.len();
            if sample_count > 0 {
                let sum: f64 = stats.latency_samples.iter().sum();
                let avg_ns = sum / sample_count as f64;
                let min_ns = stats.latency_samples.iter().cloned().fold(f64::INFINITY, f64::min);
                let max_ns = stats.latency_samples.iter().cloned().fold(0.0_f64, f64::max);
                println!(
                    "[{}s] Samples: {}, Avg: {:.3} ms, Min: {:.3} ms, Max: {:.3} ms",
                    interval,
                    sample_count,
                    avg_ns / 1_000_000.0,
                    min_ns / 1_000_000.0,
                    max_ns / 1_000_000.0
                );
            }
        }
    });

    // Wait for timeout
    loop {
        if should_stop.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(StdDuration::from_millis(100));
    }

    let _ = timeout_thread.join();
    let _ = report_thread.join();

    let mut final_stats = stats.lock().unwrap();
    let last_ns = last_message_time_ns.load(Ordering::Acquire);
    if last_ns > 0 {
        final_stats.end_time = start_instant + StdDuration::from_nanos(last_ns);
    } else {
        final_stats.end_time = final_stats.start_time;
    }

    println!("\n=== Local Latency Test Results ===");
    let duration_secs = final_stats.end_time.duration_since(final_stats.start_time).as_secs_f64();
    println!("Test duration: {:.2} seconds", duration_secs);
    println!("[INFO] Total samples: {}", final_stats.total_samples);

    if !final_stats.latency_samples.is_empty() {
        let sample_count = final_stats.latency_samples.len();
        let sum: f64 = final_stats.latency_samples.iter().sum();
        let avg_ns = sum / sample_count as f64;
        let min_ns = final_stats.latency_samples.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_ns = final_stats.latency_samples.iter().cloned().fold(0.0_f64, f64::max);

        // Calculate percentiles
        let mut sorted_samples = final_stats.latency_samples.clone();
        sorted_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());

        println!("[INFO] Latency samples: {}", sample_count);
        println!("[INFO] Average latency: {:.3} ms", avg_ns / 1_000_000.0);
        println!("[INFO] Min latency: {:.3} ms", min_ns / 1_000_000.0);
        println!("[INFO] Max latency: {:.3} ms", max_ns / 1_000_000.0);

        // CSV output
        let csv_filename = if let Some(hz) = args.common.hz {
            format!("subscriber_local_latency_results_{}b_{}hz.csv", args.common.data_len, hz)
        } else {
            format!("subscriber_local_latency_results_{}b.csv", args.common.data_len)
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
                "Timestamp,Total_Samples,Avg_Latency_ms,Min_Latency_ms,Max_Latency_ms,P50_Latency_ms,P90_Latency_ms,P99_Latency_ms"
            )
            .expect("Failed to write CSV header");
        }

        writeln!(
            csv_file,
            "{},{},{:.3},{:.3},{:.3}",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            sample_count,
            avg_ns / 1_000_000.0,
            min_ns / 1_000_000.0,
            max_ns / 1_000_000.0,
        )
        .expect("Failed to write CSV data");
    }

    let log_content = format!("Local Latency Test\nTotal samples: {}\n", final_stats.total_samples);
    let timestamp = Local::now().format("%Y%m%dT%H%M%S");
    let filename = format!("sub_local_lat-{}.log", timestamp);
    std::fs::write(&filename, log_content).expect("Failed to write log file");
}

fn main() {
    let args = SubscriberArgs::parse();

    println!("DDS Performance Test Subscriber");
    println!("Test mode: {}", args.common.test_mode);

    match args.common.test_mode.to_lowercase().as_str() {
        "throughput" | "thr" | "1" => {
            println!("Starting throughput test mode");
            run_throughput_subscriber(&args)
        }
        "latency" | "lat" | "2" => {
            println!("Starting latency test mode");
            run_latency_subscriber(&args)
        }
        "local_latency" | "local" | "ll" | "3" => {
            println!("Starting local latency test mode");
            run_local_latency_subscriber(&args)
        }
        _ => {
            println!(
                "Invalid test mode '{}'. Supported modes: throughput, latency, local_latency",
                args.common.test_mode
            );
            println!("Defaulting to throughput test.");
            run_throughput_subscriber(&args)
        }
    }
}
