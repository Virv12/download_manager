from itertools import product
from math import log2, log10
from pathlib import Path
from time import time

import matplotlib.image as img
import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns
from rich import print
from typer import run

ID = ['version', 'thread', 'segment_size', 'buffer_size']


def plot(df, const_a: str, value_a: str, const_b: str, value_b: str, x: str, y: str, look_at: str,
         look_at_repr: str = ''):
    assert all(x in ID for x in {const_a, const_b, x, y})

    dtypes_map = {
        'object': str,
        'int64': int,
        'float64': float,
    }
    type_a = dtypes_map[df.dtypes[const_a].name]
    type_b = dtypes_map[df.dtypes[const_b].name]
    df = df.loc[(df[const_a].isin([type_a(value_a), type_a('-1')])) &
                (df[const_b].isin([type_b(value_b), type_b('-1')]))]

    # Group
    df = df.groupby(ID).mean().reset_index()

    pivot_df = df.pivot(index=y, columns=x, values=look_at)
    graph = sns.heatmap(data=pivot_df, cbar=False, cmap='Blues', fmt='.2f', annot=True)
    graph.set_title(title := f'{const_a}={value_a} {const_b}={value_b} {look_at_repr}')
    plt.savefig(filename := f'graphs/{time():.0f}-{title}.png'.replace(' ', '-'))
    plt.clf()

    return filename


def visualize(i: Path, const_a: str, const_b: str, look_at: str,
              value: list[str] = None):
    value = (value if value is not None else []) + [None, None]
    value.reverse()

    # Read
    df = pd.read_csv(i, sep=',')

    # Add Data
    # df['bandwidth'] = df.apply(lambda row: 34709.942648 / row.wall_clock, axis=1)

    if look_at not in df.columns:
        look_at_repr = look_at.replace(' ', '')
        df['custom-field'] = df.apply(lambda row: eval(look_at, {}, {**row}), axis=1)
        look_at = 'custom-field'
    else:
        look_at_repr = look_at

    # Log Scaling
    df['segment_size'] = df.apply(lambda row: int(round(log2(row.segment_size))), axis=1)
    df['buffer_size'] = df.apply(lambda row: int(round(log2(row.buffer_size))) if row.buffer_size != -1 else -1, axis=1)
    df['minor_page_faults'] = df.apply(lambda row: log10(row.minor_page_faults), axis=1)
    df['voluntary_ctx_swt'] = df.apply(lambda row: log10(row.voluntary_ctx_swt), axis=1)

    value_a = value.pop()
    value_a = [value_a] if value_a is not None else df[const_a].unique().tolist()
    value_b = value.pop()
    value_b = [value_b] if value_b is not None else df[const_b].unique().tolist()

    remaining = set(ID)
    remaining.remove(const_a)
    remaining.remove(const_b)
    x, y = tuple(remaining)

    def plots():
        for parameter in product([df], [const_a], value_a, [const_b], value_b,
                                 [x], [y], [look_at], [look_at_repr]):
            print(parameter[1:])
            yield plot(*parameter)

    filenames = [*plots()]

    fig = plt.figure()
    for filename, position in zip(filenames, range(1, len(filenames) + 1)):
        fig.add_subplot(len(value_a), len(value_b), position)
        plt.imshow(img.imread(filename))
    plt.show()


if __name__ == '__main__':
    run(visualize)
