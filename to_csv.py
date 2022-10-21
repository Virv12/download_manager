"""
Command being timed: "./target/release/download_manager"
User time (seconds): 3.24
System time (seconds): 17.13
Percent of CPU this job got: 54%
Elapsed (wall clock) time (h:mm:ss or m:ss): 0:37.28
Average shared text size (kbytes): 0
Average unshared data size (kbytes): 0
Average stack size (kbytes): 0
Average total size (kbytes): 0
Maximum resident set size (kbytes): 14476
Average resident set size (kbytes): 0
Major (requiring I/O) page faults: 0
Minor (reclaiming a frame) page faults: 18542
Voluntary context switches: 2977583
Involuntary context switches: 293
Swaps: 0
File system inputs: 0
File system outputs: 8474256
Socket messages sent: 0
Socket messages received: 0
Signals delivered: 0
Page size (bytes): 4096
Exit status: 0
"""
import sys

from pathlib import Path
from typer import run
from re import match

FIELDS = {
    'version': str,
    'thread': int,
    'segment_size': int,
    'iteration': int,
    'usr_time': float,
    'sys_time': float,
    'cpu%': lambda x: float(x[:-1]) / 100,
    'wall_clock': lambda x: sum(float(v) * s for v, s in zip(reversed(x.split(':')), [1, 60, 3600])),
    'avg_shared_text': lambda x: int(x) * 1000,
    'avg_unshared_data': lambda x: int(x) * 1000,
    'avg_stack': lambda x: int(x) * 1000,
    'avg_total': lambda x: int(x) * 1000,
    'max_resident_set': lambda x: int(x) * 1000,
    'avg_resident_set': lambda x: int(x) * 1000,
    'major_page_faults': int,
    'minor_page_faults': int,
    'voluntary_ctx_swt': int,
    'involuntary_ctx_swt': int,
    'swaps': int,
    'fs_input': int,
    'fs_output': int,
    'socket_sent': int,
    'socket_received': int,
    'signals_delivered': int,
    'page_size': int,
    'exit_status': int,
}

HASH_MAP = {
    '8d56af8a13b288f5df52110e7564f99981b2cc85': 'read-write'
}


def to_csv(root: Path, output: Path):
    def parse(result: Path):
        yield HASH_MAP[result.parent.name]
        yield from result.name.split('_')
        yield from [match(r'.+?: (.+)', s.strip()).group(1)
                    for s in result.read_text().splitlines()][1:]

    with output.open('w') as output:
        output.write(','.join(FIELDS.keys()) + '\n')
        for result in root.resolve().rglob('*'):
            try:
                parsed = [deserialize(v) for v, deserialize in zip(parse(result), FIELDS.values())]
                assert len(parsed) == len(FIELDS.keys())
                output.write(','.join(map(str, parsed)) + '\n')
            except Exception as e:
                print(e)
                print(result)


if __name__ == '__main__':
    run(to_csv)
