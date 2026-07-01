use std::path::Path;

fn main() {
    let artifact_path = Path::new("../vaughan-contracts/out/AmbireAccount.sol/AmbireAccount.json");

    if artifact_path.exists() {
        let json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(artifact_path).expect("read artifact"),
        )
        .expect("parse artifact JSON");

        let bytecode_hex = json["bytecode"]["object"]
            .as_str()
            .expect("bytecode.object field missing")
            .trim_start_matches("0x");

        let bytecode_bytes = hex::decode(bytecode_hex).expect("decode hex bytecode");

        let out_dir = std::env::var("OUT_DIR").unwrap();
        let out_path = Path::new(&out_dir).join("ambire_account_bytecode.bin");
        std::fs::write(&out_path, &bytecode_bytes).expect("write bytecode bin");

        println!("cargo:rerun-if-changed=../vaughan-contracts/out/AmbireAccount.sol/AmbireAccount.json");
    } else {
        // Write empty file so compilation doesn't fail before forge build
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let out_path = Path::new(&out_dir).join("ambire_account_bytecode.bin");
        std::fs::write(&out_path, []).expect("write empty bytecode");
        println!("cargo:warning=AmbireAccount artifact not found — run `forge build` in vaughan-contracts/");
    }
}
