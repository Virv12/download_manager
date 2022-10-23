#!/bin/sh

mkdir -p bench/data

for FEATURE in '' splice_single splice_double; do
    for NUM_THREADS in 64 32 16 8 4 2 1; do
        for SEGMENT_SIZE_POW in $(seq 17 29); do
			for BUFFER_SIZE_POW in $(seq 12 20); do
				export NUM_THREADS
				export SEGMENT_SIZE=$(( 1 << $SEGMENT_SIZE_POW ))
				export BUFFER_SIZE=$(( 1 << $BUFFER_SIZE_POW ))
				cargo build --release --features="$FEATURE"
				HASH=$(sha1sum target/release/download_manager | awk '{ print $1 }')
				for i in $(seq 0 2); do
					FILE=data/${HASH}_$i
					if ! [[ -f bench/$FILE ]]; then
						echo $FEATURE $NUM_THREADS $SEGMENT_SIZE $BUFFER_SIZE $i
						sleep 0.5
						/bin/time -vvv ./target/release/download_manager <list.txt 2>/tmp/download_manager_bench && mv /tmp/download_manager_bench bench/$FILE
					fi
					if [[ "$FEATURE" == "" ]]; then
						ln -sf $FILE bench/default-${NUM_THREADS}-${SEGMENT_SIZE}-${BUFFER_SIZE}-$i
					else
						ln -sf $FILE bench/${FEATURE}-${NUM_THREADS}-${SEGMENT_SIZE}-$i
					fi
				done
				if [[ "$FEATURE" != "" ]]; then
					break
				fi
            done
        done
    done
done
