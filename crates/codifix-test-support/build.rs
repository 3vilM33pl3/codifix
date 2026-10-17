// SPDX-License-Identifier: AGPL-3.0-or-later
fn main() {
    // sqlx::migrate! alone cannot reliably notice a newly added migration file.
    println!("cargo:rerun-if-changed=../../migrations");
}
