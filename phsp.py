import logging
import struct
import argparse
from collections import OrderedDict

# argparse.
logger = logging.getLogger(__name__)

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


def write(fname, header, records):
    logger.debug('Writing to %s', fname)
    f = open(fname, 'wb')

    if records and 'zlast' in records[0]:
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
    for record in records:
        f.write(struct.pack(record_format, *record.values()))
    logger.debug('Finished writing %s records', len(records))


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
    record_size = struct.calcsize(record_format)
    records = []
    for i in range(header['total_particles']):
        record_bytes = f.read(record_size)
        record_values = struct.unpack(record_format, record_bytes)
        record = OrderedDict(zip(record_fields.keys(), record_values))
        logger.debug('Read record %s', record)
        records.append(record)
    extra_bytes = f.read()
    logger.debug('Finished reading, %s bytes left at end of file', len(extra_bytes))
    return header, records

if __name__ == '__main__':
    logging.basicConfig(level=logging.DEBUG)
    fname = '/Users/henry/projects/EGSnrc/egs_home/BEAM_TUMOTRAK/input.egsphsp1'
    header, records = read(fname)
    fname = '/Users/henry/projects/EGSnrc/egs_home/BEAM_TUMOTRAK/input.egsphsp2'
    write(fname, header, records)
