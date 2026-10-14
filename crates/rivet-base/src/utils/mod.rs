//! Common ecosystem utilities: time, IDs, UTF-8 and endianness.

pub mod bytes;
pub mod clock;
pub mod ids;
pub mod utf8;

pub use bytes::{from_le_f32, from_le_u16, from_le_u32, to_le_u16, to_le_u32};
pub use clock::{Clock, MockClock, SystemClock};
pub use ids::{id_generation, id_index, make_id};
pub use utf8::truncate_utf8;
