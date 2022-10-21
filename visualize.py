from pathlib import Path
from typer import run
from rich import print

import pandas as pd


def visualize(csv: Path):
    df = pd.read_csv(csv, sep=',')
    df = df.filter([
        'version', 'thread', 'segment_size',
        'usr_time', 'sys_time', 'cpu%', 'wall_clock',
    ])
    df['bandwidth'] = df.apply(lambda row: 34709.942648 / row.wall_clock, axis=1)

    print(df.groupby(['version', 'thread', 'segment_size']).mean())


if __name__ == '__main__':
    run(visualize)
