import logging
import struct
from collections import OrderedDict

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


def make_latch(npass, iq, brem_or_positron, bit_region, bit_region_number):
    latch = 0
    if npass:
        latch |= 1 << 31
    if iq == -1:
        latch |= 1 << 30
    elif iq == 1:
        latch |= 1 << 29
    if brem_or_positron:
        latch |= 1
    latch |= bit_region & 0xffffff - 1
    latch |= bit_region_number << 24 & 0xf000000


def parse_latch(latch):
    npass = latch & 1 << 31
    if latch & 1 << 30:
        iq = -1
    elif latch & 1 << 29:
        iq = 1
    else:
        iq = 0
    brem_or_positron = latch & 1
    bit_region = latch & 0xffffff - 1
    bit_region_number = (latch & 0xf000000) >> 24
    return npass, iq, brem_or_positron, bit_region, bit_region_number

    """
    see GET_LATCHTMP_ESHORT_WEIGHTTMP

    bit 0 Set to 1 if a bremsstrahlung or positron annihilation event occurs in the history; 0
        otherwise(not used for LATCH_OPTION = 1).

    bit 1-23 Used to record the bit region where a particle has been and/or has interacted
        (Note that the bit set for a region is determined by IREGION_TO_BIT for that region)

    bit 24-28 Stores the bit region number (as opposed to geometric region) in which a secondary
        particle is created; if these bits are all 0, the particle is a primary particle (not
        for LATCH_OPTION = 1).

    bit 29-30 Store the charge of a particle when LATCH is output to a phase space file (see
        section 7 on phase space files). During a simulation, bit 30 is used to identify a
        contaminant particle but this information is not output to the phase space file. Set
        to 1 if the particle is a contaminant particle; 0 otherwise. Note that if LATCH is not
        inherited (i.e. when LATCH_OPTION = 1), bit 30 loses its meaning.

    bit 31 Set to 1 if a particle has crossed a scoring plane more than once when LATCH is output
        to a phase space file (see section 7 on phase space files above)
"""
    pass


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
