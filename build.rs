use std::{env, fs, path::Path};
use uuid::Uuid;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let secret_path = Path::new(&out_dir).join("secret.uuid");
    println!("Generating deployment secret to {}...", secret_path.display());

    let mut secret: String;

    if !secret_path.exists() {
        secret = Uuid::new_v4().to_string();
        fs::write(&secret_path, &secret).unwrap();
    }

    secret = fs::read_to_string(secret_path).unwrap();

    println!("DEPLOY_SECRET={}", secret);
}
