#int2dds_subscriber.sh
#!/bin/bash

REPEAT_COUNT=10

# Test configurations
declare -a TESTS=(
    "latency 1048576 60"
    "throughput 1048576 60"
    "latency 65536 700"
    "throughput 65536 700"
    "latency 1024 1000"
    "throughput 1024 1000"
)

LOGFILE="./subscriber.log"

for test_config in "${TESTS[@]}"
do
    read -r test_mode data_len hz <<< "$test_config"
    echo "========================================="
    echo "Running $test_mode test with data-len $data_len"
    echo "========================================="

    rm -rf "$LOGFILE"

    for i in $(seq 1 $REPEAT_COUNT)
    do

        echo "Starting perftest_subscriber (Run $i/$REPEAT_COUNT) at $(date '+%Y-%m-%d %H:%M:%S')"
        echo "Starting perftest_subscriber $test_mode (Run $i/$REPEAT_COUNT) $data_len bytes $hz hz" 2>&1 | tee -a "$LOGFILE"

        start_time=$(date +%s)

        ./perftest_subscriber --execution-time 30 --test-mode $test_mode --data-len $data_len --hz $hz 2>&1 | tee -a "$LOGFILE"

        end_time=$(date +%s)
        elapsed=$((end_time - start_time))
        wait_time=$((40 - elapsed))

        if [ $wait_time -gt 0 ]; then
            echo "Waiting $wait_time seconds before next run..."
            sleep $wait_time
        fi

    done
done


echo "Completed all tests"
