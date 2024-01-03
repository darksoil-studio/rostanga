use holochain_types::prelude::{holochain_serial, AnyDhtHashB64, DnaHashB64, SerializedBytes};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, SerializedBytes)]
pub struct HrlBody {
    pub dna_hash: DnaHashB64,
    pub dht_hash: AnyDhtHashB64,
}
