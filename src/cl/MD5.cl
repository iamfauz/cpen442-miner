// MD5 Kernel

/* The basic MD5 functions */
#define F(x, y, z)			((z) ^ ((x) & ((y) ^ (z))))
#define G(x, y, z)			((y) ^ ((z) & ((x) ^ (y))))
#define H(x, y, z)			((x) ^ (y) ^ (z))
#define I(x, y, z)			((y) ^ ((x) | ~(z)))

/* The MD5 transformation for all four rounds. */
#define STEP(f, a, b, c, d, x, t, s) \
    (a) += f((b), (c), (d)) + (x) + (t); \
    (a) = (((a) << (s)) | (((a) & 0xffffffff) >> (32 - (s)))); \
    (a) += (b);

#define GET(i) (message[(i)])

// void md5_round(uint* internal_state, const uint* message);
// Credit to https://github.com/awreece/pdfcrack-opencl for the MD5 algorithm
static inline void md5_round(uint* internal_state, const uint* message) {
  uint a, b, c, d;
  a = internal_state[0];
  b = internal_state[1];
  c = internal_state[2];
  d = internal_state[3];

  /* Round 1 */
  STEP(F, a, b, c, d, GET(0), 0xd76aa478, 7)
  STEP(F, d, a, b, c, GET(1), 0xe8c7b756, 12)
  STEP(F, c, d, a, b, GET(2), 0x242070db, 17)
  STEP(F, b, c, d, a, GET(3), 0xc1bdceee, 22)
  STEP(F, a, b, c, d, GET(4), 0xf57c0faf, 7)
  STEP(F, d, a, b, c, GET(5), 0x4787c62a, 12)
  STEP(F, c, d, a, b, GET(6), 0xa8304613, 17)
  STEP(F, b, c, d, a, GET(7), 0xfd469501, 22)
  STEP(F, a, b, c, d, GET(8), 0x698098d8, 7)
  STEP(F, d, a, b, c, GET(9), 0x8b44f7af, 12)
  STEP(F, c, d, a, b, GET(10), 0xffff5bb1, 17)
  STEP(F, b, c, d, a, GET(11), 0x895cd7be, 22)
  STEP(F, a, b, c, d, GET(12), 0x6b901122, 7)
  STEP(F, d, a, b, c, GET(13), 0xfd987193, 12)
  STEP(F, c, d, a, b, GET(14), 0xa679438e, 17)
  STEP(F, b, c, d, a, GET(15), 0x49b40821, 22)

  /* Round 2 */
  STEP(G, a, b, c, d, GET(1), 0xf61e2562, 5)
  STEP(G, d, a, b, c, GET(6), 0xc040b340, 9)
  STEP(G, c, d, a, b, GET(11), 0x265e5a51, 14)
  STEP(G, b, c, d, a, GET(0), 0xe9b6c7aa, 20)
  STEP(G, a, b, c, d, GET(5), 0xd62f105d, 5)
  STEP(G, d, a, b, c, GET(10), 0x02441453, 9)
  STEP(G, c, d, a, b, GET(15), 0xd8a1e681, 14)
  STEP(G, b, c, d, a, GET(4), 0xe7d3fbc8, 20)
  STEP(G, a, b, c, d, GET(9), 0x21e1cde6, 5)
  STEP(G, d, a, b, c, GET(14), 0xc33707d6, 9)
  STEP(G, c, d, a, b, GET(3), 0xf4d50d87, 14)
  STEP(G, b, c, d, a, GET(8), 0x455a14ed, 20)
  STEP(G, a, b, c, d, GET(13), 0xa9e3e905, 5)
  STEP(G, d, a, b, c, GET(2), 0xfcefa3f8, 9)
  STEP(G, c, d, a, b, GET(7), 0x676f02d9, 14)
  STEP(G, b, c, d, a, GET(12), 0x8d2a4c8a, 20)

  /* Round 3 */
  STEP(H, a, b, c, d, GET(5), 0xfffa3942, 4)
  STEP(H, d, a, b, c, GET(8), 0x8771f681, 11)
  STEP(H, c, d, a, b, GET(11), 0x6d9d6122, 16)
  STEP(H, b, c, d, a, GET(14), 0xfde5380c, 23)
  STEP(H, a, b, c, d, GET(1), 0xa4beea44, 4)
  STEP(H, d, a, b, c, GET(4), 0x4bdecfa9, 11)
  STEP(H, c, d, a, b, GET(7), 0xf6bb4b60, 16)
  STEP(H, b, c, d, a, GET(10), 0xbebfbc70, 23)
  STEP(H, a, b, c, d, GET(13), 0x289b7ec6, 4)
  STEP(H, d, a, b, c, GET(0), 0xeaa127fa, 11)
  STEP(H, c, d, a, b, GET(3), 0xd4ef3085, 16)
  STEP(H, b, c, d, a, GET(6), 0x04881d05, 23)
  STEP(H, a, b, c, d, GET(9), 0xd9d4d039, 4)
  STEP(H, d, a, b, c, GET(12), 0xe6db99e5, 11)
  STEP(H, c, d, a, b, GET(15), 0x1fa27cf8, 16)
  STEP(H, b, c, d, a, GET(2), 0xc4ac5665, 23)

  /* Round 4 */
  STEP(I, a, b, c, d, GET(0), 0xf4292244, 6)
  STEP(I, d, a, b, c, GET(7), 0x432aff97, 10)
  STEP(I, c, d, a, b, GET(14), 0xab9423a7, 15)
  STEP(I, b, c, d, a, GET(5), 0xfc93a039, 21)
  STEP(I, a, b, c, d, GET(12), 0x655b59c3, 6)
  STEP(I, d, a, b, c, GET(3), 0x8f0ccc92, 10)
  STEP(I, c, d, a, b, GET(10), 0xffeff47d, 15)
  STEP(I, b, c, d, a, GET(1), 0x85845dd1, 21)
  STEP(I, a, b, c, d, GET(8), 0x6fa87e4f, 6)
  STEP(I, d, a, b, c, GET(15), 0xfe2ce6e0, 10)
  STEP(I, c, d, a, b, GET(6), 0xa3014314, 15)
  STEP(I, b, c, d, a, GET(13), 0x4e0811a1, 21)
  STEP(I, a, b, c, d, GET(4), 0xf7537e82, 6)
  STEP(I, d, a, b, c, GET(11), 0xbd3af235, 10)
  STEP(I, c, d, a, b, GET(2), 0x2ad7d2bb, 15)
  STEP(I, b, c, d, a, GET(9), 0xeb86d391, 21)

  internal_state[0] = a + internal_state[0];
  internal_state[1] = b + internal_state[1];
  internal_state[2] = c + internal_state[2];
  internal_state[3] = d + internal_state[3];
}

// IMPORTANT: all lengths used are word lengths (32 bit)!

// Use an exact multiple to avoid padding
#ifndef MESSAGE_LEN
#define MESSAGE_LEN (256 / 4)
#endif // MESSAGE_LEN

// Length of a MD5 sum encoded as hex
#define _MD5_HEX_LEN (32 / 4)

//      11112222333344445555
// len "CPEN 442 Coin2019"
#define _COIN_PREFIX_LEN (5)
#define _PREV_COIN_LEN (_MD5_HEX_LEN)
#define _TRACKER_ID_LEN (_MD5_HEX_LEN)

// Start of the modifiable part of the message
#ifndef BLOB_INDEX
#define BLOB_INDEX (_COIN_PREFIX_LEN + _PREV_COIN_LEN)
#endif // BLOB_INDEX

// Length of the modifiable part of the message
#ifndef BLOB_LEN
// 64 - 8 - 8 - 5 = 43
#define BLOB_LEN (MESSAGE_LEN - _TRACKER_ID_LEN - BLOB_INDEX)
#endif

// Largest power of 2 that fits in BLOB_LEN for performance
#ifndef BLOB_LEN_FAST
#define BLOB_LEN_FAST 32
#endif // BLOB_LEN_FAST

// The index of the counter
// This must be in the last round of MD5 for it to generate
// more unique hashes
#ifndef LAST_ROUND_COUNTER_INDEX
#define LAST_ROUND_COUNTER_INDEX (192 / 4)
#endif

// Number of loops to do
#ifndef N_LOOPS
#define N_LOOPS 4096
#endif

// Number of inner (fast) loops to do
#ifndef N_LOOPS_2
#define N_LOOPS_2 256
#endif

// Check endianess as this program is only good for little endian
#ifndef __LITTLE_ENDIAN__
#error This kernel currently only supports little endian architectures!
#endif

/**
 * Parallel MD5 hash kernel
 *
 * The purpose of this kernel is to parallelize the computation
 * of many *slightly* different strings to find MD5 sums
 * of a certain 32 bit prefix (In this case all 0s).
 *
 * Each instance of this function modifies a base random message
 * depending on its global id and then MD5 hashes it then returns
 * the 32 bit prefix.
 *
 * In host post processing one can reproduce the message
 *
 */
__kernel void md5(
    // The base message to hash
    __constant uint* base_message,
    // Extra random values
    __constant uint* params_in,
    // The parameters output to the program
    // Note that many processing units may try to write to this location
    __global uint* params_out) {
  uint i;
  uint j;
  const uint id = get_global_id(0);
  uint message[MESSAGE_LEN];
  uint zero_pad[16];
  uint r0 = params_in[0];
  uint r1 = params_in[1];
  uint r2 = params_in[2];
  uint r3 = params_in[3];

  // Set most significant bit of first pad byte
  zero_pad[0] = 0x80;
  // Fill with zeroes
  for (i = 1; i < 14; ++i) {
    zero_pad[i] = 0;
  }
  // 8 byte integer with the length of the message in bits
  zero_pad[14] = MESSAGE_LEN * 32;
  zero_pad[15] = 0;

  // Copy message locally (Probably quite slow)
  for (i = 0; i < MESSAGE_LEN; ++i) {
    message[i] = base_message[i];
  }

  uint orig0 = message[BLOB_INDEX + (id + r0) % BLOB_LEN_FAST];
  uint orig1 = message[BLOB_INDEX + (id + r1 + BLOB_LEN_FAST / 4) % BLOB_LEN_FAST];
  uint orig2 = message[BLOB_INDEX + BLOB_LEN_FAST];
  uint orig3 = message[LAST_ROUND_COUNTER_INDEX];

  uint md5_state[4];
  uint md5_state_2[4];

  for (i = 0; i < N_LOOPS; ++i) {
    // Initialize MD5
    md5_state[0] = 0x67452301;
    md5_state[1] = 0xefcdab89;
    md5_state[2] = 0x98badcfe;
    md5_state[3] = 0x10325476;

    // Modify the message per iteration based on ID
    message[BLOB_INDEX + (id + r0) % BLOB_LEN_FAST] = orig0 + id + i * 4;
    message[BLOB_INDEX + (id + r1 + BLOB_LEN_FAST / 4) % BLOB_LEN_FAST] = orig1 ^ ((id << 16) | id);
    message[BLOB_INDEX + BLOB_LEN_FAST] = orig2 + (id << 16) + i - r2;

    // Perform MD5 till before the last round
    for (j = 0; j < MESSAGE_LEN / 16 - 1; ++j) {
      md5_round(md5_state, &message[j * 16]);
    }

    for (j = 0; j < N_LOOPS_2; ++j) {
      // Don't clobber our state
      md5_state_2[0] = md5_state[0];
      md5_state_2[1] = md5_state[1];
      md5_state_2[2] = md5_state[2];
      md5_state_2[3] = md5_state[3];

      message[LAST_ROUND_COUNTER_INDEX] = (orig3 + (j >> 2) + (j << 24) + (i << 12)) ^ r3;

      // Perform the last 2 rounds of MD5
      md5_round(md5_state_2, &message[MESSAGE_LEN - 16]);
      md5_round(md5_state_2, zero_pad);

      // Note skip the padding algorithm since the
      // message is already a multiple of the block size

      // Output our prefix
      if (md5_state_2[0] == 0 && params_out[0] == 0xFFFFFFFF) {
        params_out[0] = id;
        params_out[1] = i;
        params_out[2] = j;

#ifdef __DEBUG_MODE__
        params_out[3] = md5_state_2[0];
        params_out[4] = md5_state_2[1];
        params_out[5] = md5_state_2[2];
        params_out[6] = md5_state_2[3];

        for (i = 0; i < MESSAGE_LEN; ++i) {
          params_out[7 + i] = message[i];
        }
#endif
        break;
      }
    }
  }
}
