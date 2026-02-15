#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use scheng_graph::NodeId;
    use scheng_runtime::runtime_contract::plan_output_names;
    use scheng_runtime::BankSet;

    // ---- Golden fixtures (JSON contracts) ----
    const BANKS_BUILTIN_JSON: &str = include_str!("../fixtures/banks_builtin.json");
    const BANKS_BAD_PRESET_JSON: &str = include_str!("../fixtures/banks_bad_preset.json");
    const BANKS_EMPTY_JSON: &str = include_str!("../fixtures/banks_empty.json");
    const BANKS_MISSING_KEY_JSON: &str = include_str!("../fixtures/banks_missing_key.json");

    fn write_temp_fixture(name: &str, contents: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        p.push(format!("scheng_contract_tests_{name}_{ts}.json"));
        fs::write(&p, contents).expect("write fixture");
        p
    }

    #[test]
    fn golden_banks_builtin_json_deserializes() {
        let path = write_temp_fixture("banks_builtin", BANKS_BUILTIN_JSON);

        let banks = BankSet::from_json_path(&path).expect("banks_builtin.json should parse");
        assert!(!banks.banks.is_empty(), "builtin banks should not be empty");

        // Keep stable but not overly strict: ensure at least one bank has at least one scene.
        let any_scene = banks.banks.iter().any(|b| !b.scenes.is_empty());
        assert!(any_scene, "expected at least one scene in builtin banks");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn golden_banks_empty_is_rejected() {
        let path = write_temp_fixture("banks_empty", BANKS_EMPTY_JSON);

        let err = BankSet::from_json_path(&path)
            .expect_err("banks_empty.json must fail (empty banks)");

        // Keep this stable but not overly strict.
        assert!(
            err.to_lowercase().contains("banks") || err.to_lowercase().contains("empty"),
            "expected error to mention banks/empty, got: {err}"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn golden_banks_missing_key_is_rejected() {
        let path = write_temp_fixture("banks_missing_key", BANKS_MISSING_KEY_JSON);

        let err = BankSet::from_json_path(&path)
            .expect_err("banks_missing_key.json must fail (missing key)");

        // Keep this stable but not overly strict.
        assert!(
            err.to_lowercase().contains("missing") || err.to_lowercase().contains("key"),
            "expected error to mention missing/key, got: {err}"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn golden_banks_unknown_preset_is_rejected() {
        let path = write_temp_fixture("banks_bad_preset", BANKS_BAD_PRESET_JSON);

        let err = BankSet::from_json_path(&path)
            .expect_err("banks_bad_preset.json must fail (unknown preset)");

        // Keep this stable but not overly strict.
        assert!(
            err.to_lowercase().contains("unknown preset"),
            "expected error to mention 'unknown preset', got: {err}"
        );

        let _ = fs::remove_file(path);
    }

    // ---- Step 5.1 output naming contracts (backend-agnostic) ----

    #[test]
    fn outputs_rejects_zero_pixels_out() {
        let err = plan_output_names(&[]).expect_err("must reject graphs with no PixelsOut");
        assert!(
            err.to_lowercase().contains("no pixelsout"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn outputs_rejects_two_unnamed_pixels_out() {
        let err = plan_output_names(&[(NodeId(1), None), (NodeId(2), None)])
            .expect_err("must reject 2 unnamed PixelsOut (ambiguous primary)");
        assert!(
            err.to_lowercase().contains("exactly 1 unnamed"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn outputs_rejects_reserved_main_name() {
        let err = plan_output_names(&[(NodeId(1), None), (NodeId(2), Some("main"))])
            .expect_err("must reject explicit name 'main'");
        assert!(
            err.to_lowercase().contains("reserved"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn outputs_rejects_duplicate_named_outputs() {
        let err = plan_output_names(&[
            (NodeId(1), None),
            (NodeId(2), Some("program")),
            (NodeId(3), Some("program")),
        ])
        .expect_err("must reject duplicate explicit output names");
        assert!(
            err.to_lowercase().contains("duplicate output name"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn outputs_accepts_one_unnamed_plus_two_named() {
        let plan = plan_output_names(&[
            (NodeId(10), None),
            (NodeId(11), Some("program")),
            (NodeId(12), Some("preview")),
        ])
        .expect("should accept 1 unnamed + N named");

        assert_eq!(plan.primary, NodeId(10));
        assert_eq!(plan.named.get("program").copied(), Some(NodeId(11)));
        assert_eq!(plan.named.get("preview").copied(), Some(NodeId(12)));
    }
}

#[cfg(test)]
mod determinism;
