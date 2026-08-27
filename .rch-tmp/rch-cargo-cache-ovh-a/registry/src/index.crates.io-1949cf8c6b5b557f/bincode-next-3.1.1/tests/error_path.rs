use bincode_next::config;
use bincode_next::de::Decoder;
use bincode_next::error::DecodeError;
use bincode_next::error_path::BincodeErrorPathCovered;

struct CustomDecoder;

impl bincode_next::utils::Sealed for CustomDecoder {}
impl BincodeErrorPathCovered<0> for CustomDecoder {}

impl Decoder for CustomDecoder {
    type C = config::Configuration<config::LittleEndian, config::Varint, config::NoLimit>;
    type Context = ();
    type R = bincode_next::de::read::SliceReader<'static>;

    fn reader(&mut self) -> &mut Self::R {
        unreachable!()
    }

    fn config(&self) -> &Self::C {
        unreachable!()
    }

    fn context(&mut self) -> &mut Self::Context {
        unreachable!()
    }

    fn claim_bytes_read(
        &mut self,
        _n: usize,
    ) -> Result<(), DecodeError> {
        Self::assert_covered();
        Ok(())
    }

    fn unclaim_bytes_read(
        &mut self,
        _n: usize,
    ) {
    }
}

#[test]
fn test_covered_path() {
    let mut decoder = CustomDecoder;
    decoder.claim_bytes_read(10).unwrap();
}

// This test would fail to compile if we uncomment it and BincodeErrorPathCovered<1> is not implemented for CustomDecoder
// #[test]
// fn test_uncovered_path() {
// <CustomDecoder as BincodeErrorPathCovered<1>>::assert_covered();
// }
