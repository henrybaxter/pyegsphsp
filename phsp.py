import sys
import logging
import struct
import argparse
import itertools
from collections import OrderedDict, namedtuple

# argparse.
logger = logging.getLogger(__name__)

CHUNK_SIZE = 64 * 1024

HEADER_FIELDS = OrderedDict((
    ('total_particles', 'i'),  # NUM_PHSP_TOT
    ('total_photons', 'i'),  # PHOT_PHSP_TOT
    ('max_energy', 'f'),  # EKMAX_PHSP_SHORT
    ('min_energy', 'f'),  # EKMINE_PHSP_SHORT
    ('total_particles_in_source', 'f')  # NINC_PHSP_SHORT
))
HEADER_FORMAT = ''.join(HEADER_FIELDS.values())
HEADER_SIZE = struct.calcsize(HEADER_FORMAT)

RECORD_FIELDS = OrderedDict((
    ('latch', 'I'),
    ('total_energy', 'f'),  # ENERGY, negative marks
    # first particle scored by each new primary history
    # (see DOSXYZ manual page 76)
    ('x_cm', 'f'),  # X
    ('y_cm', 'f'),  # Y
    ('x_cos', 'f'),  # U
    ('y_cos', 'f'),  # Y
    ('weight', 'f'),  # WT, also carries sign of z direction (!)
))
RECORD_FIELDS_ZLAST = RECORD_FIELDS.copy()
RECORD_FIELDS_ZLAST['zlast'] = 'f'

MODE0 = b'MODE0'
MODE2 = b'MODE2'


def translate(fname, x, y):
    f = open(fname, 'r+b')
    mode = f.read(5)
    if mode == MODE0:
        record_length = 28
    elif mode == MODE2:
        record_length = 32
    else:
        raise ValueError('Unknown mode {}'.format(repr(mode)))
    total_particles = struct.unpack('i', f.read(4))[0]
    print('total particles', total_particles)
    XY_OFFSET = 8
    for i in range(total_particles):
        index = (i + 1) * record_length + XY_OFFSET
        f.seek(index)
        x, y = struct.unpack('ff', f.read(8))
        x += x
        y += y
        f.seek(index)
        f.write(struct.pack('ff', x, y))
    print('done')


def write(fname, header, records, zlast=False):
    logger.debug('Writing to %s', fname)
    logger.debug('Header is %s', header)
    f = open(fname, 'wb')
    if zlast:
        f.write(b'MODE2')
        record_fields = RECORD_FIELDS_ZLAST
        header_padding = b'\0' * 7
    else:
        f.write(b'MODE0')
        record_fields = RECORD_FIELDS
        header_padding = b'\0' * 3
    f.write(struct.pack(HEADER_FORMAT, *header.values()))
    # pad with null bytes so header is correct length
    f.write(header_padding)
    record_format = ''.join(record_fields.values())
    for i in range(header['total_particles'] // CHUNK_SIZE + 1):
        f.write(b''.join((struct.pack(record_format, *record) for record in itertools.islice(records, CHUNK_SIZE))))
    logger.debug('Finished writing')


def read(fname):
    logger.debug('Reading %s', fname)
    f = open(fname, 'rb')
    mode_bytes = f.read(5)
    if mode_bytes == b'MODE0':
        logger.debug('Found MODE0 file')
        record_fields = RECORD_FIELDS
        header_padding = 3
    elif mode_bytes == b'MODE2':
        logger.debug('Found MODE2 file')
        record_fields = RECORD_FIELDS_ZLAST
        header_padding = 7
    else:
        raise ValueError('First 5 bytes must specify mode (MODE0 or MODE2)')
    header_bytes = f.read(HEADER_SIZE + header_padding)[:HEADER_SIZE]
    header_values = struct.unpack(HEADER_FORMAT, header_bytes)
    header = OrderedDict(zip(HEADER_FIELDS.keys(), header_values))
    logger.debug('Found header %s', header)
    record_format = ''.join(record_fields.values())
    record_length = struct.calcsize(record_format)
    Record = namedtuple('Record', record_fields.keys())

    def read_records():
        for i in range(header['total_particles'] // CHUNK_SIZE + 1):
            buffer = f.read(record_length * CHUNK_SIZE)
            for j in range(0, len(buffer), record_length):
                record = Record(*struct.unpack(record_format, buffer[j:j + record_length]))
                yield record
            logger.debug('Read %s records', (i + 1) * CHUNK_SIZE)
        # extra_bytes = f.read()
        # logger.debug('Finished reading, %s bytes left at end of file', len(extra_bytes))
    return header, read_records()


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument('input', nargs='+')
    parser.add_argument('output')
    parser.add_argument('--translate-x', '-tx', default=0, type=float, help='Translate phase space by tx centimeters')
    parser.add_argument('--translate-y', '-ty', default=0, type=float, help='Translate phase space by ty centimeters')
    return parser.parse_args()


def combine(fnames):
    # to combine, we simply add the records and update the header
    combined_header = OrderedDict((
        ('total_particles', 0),
        ('total_photons', 0),
        ('max_energy', 0.0),
        ('min_energy', 100.0),
        ('total_particles_in_source', 0.0),
    ))
    records_iterators = []
    for fname in fnames:
        header, records = read(fname)
        combined_header['total_particles'] += header['total_particles']
        combined_header['total_photons'] += header['total_photons']
        combined_header['max_energy'] = max(combined_header['max_energy'], header['max_energy'])
        combined_header['min_energy'] = min(combined_header['min_energy'], header['min_energy'])
        combined_header['total_particles_in_source'] += header['total_particles_in_source']
        records_iterators.append(records)
    if combined_header['min_energy'] == 100.0:
        logger.warning('Minimum energy {}'.format(combined_header['min_energy']))
    return combined_header, itertools.chain(*records_iterators)


if __name__ == '__main__':
    logging.basicConfig(level=logging.DEBUG)
    args = parse_args()
    if args.translate_x or args.translate_y:
        print('translating in place')
        translate(args.input[0], args.translate_x, args.translate_y)
        sys.exit()
    if len(args.input) > 1:
        print('Cannot combine less than 2 phase space files')
        sys.exit(1)
        header, records = combine(args.input)
    else:
        print('just reading one')
        header, records = read(args.input[0])
    if args.translate_x or args.translate_y:
        def translate(records):
            for r in records:
                yield r._replace(x_cm=r.x_cm + args.translate_x, y_cm=r.y_cm + args.translate_y)
        records = translate(records)
    write(args.output, header, records)
