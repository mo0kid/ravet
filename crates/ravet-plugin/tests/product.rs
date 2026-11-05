use ravet_foundation::{AudioLayout, ParameterId};
use ravet_plugin::{
    Category, IdentityOverrides, NotePorts, Parameter, Product, ProductError, StateFormat,
    derive_parameter_id,
};

const FORMAT: StateFormat = StateFormat {
    tag: *b"EXGAIN01",
    version: 1,
};
const PARAMETERS: &[Parameter] = &[
    Parameter::linear("gain_db", "Gain", -60.0, 12.0, 0.0).with_unit("dB"),
    Parameter::bypass("bypass", "Bypass"),
    Parameter::logarithmic("cutoff", "Cutoff", 20.0, 20_000.0, 1_000.0).with_id(7),
];
const GAIN: Product = Product::new(
    "Example Gain",
    "Example Audio",
    "1.0.0",
    "com.example.gain",
    Category::Effect,
    FORMAT,
)
.with_layouts(&[AudioLayout::STEREO, AudioLayout::MONO])
.with_parameters(PARAMETERS);

fn effect(parameters: &'static [Parameter<'static>]) -> Product<'static> {
    GAIN.with_parameters(parameters)
}

#[test]
fn parameter_ids_are_explicit_or_derived_from_keys_never_from_order() {
    let product = GAIN.resolve().unwrap();
    // Pinned derivation: 32-bit FNV-1a of the key with the top bit cleared.
    assert_eq!(derive_parameter_id("gain_db"), ParameterId(0x046c_1bf5));
    assert_eq!(derive_parameter_id("bypass"), ParameterId(0x5dcb_1cad));
    assert_eq!(
        product.parameter_ids(),
        [
            ParameterId(0x046c_1bf5),
            ParameterId(0x5dcb_1cad),
            ParameterId(7)
        ]
    );
    assert_eq!(product.bypass_index(), Some(1));
    assert_eq!(product.parameter_index(ParameterId(7)), Some(2));
    assert_eq!(product.parameter_index_by_key("bypass"), Some(1));
    assert_eq!(product.defaults(), [0.0, 0.0, 1_000.0]);

    const REORDERED: &[Parameter] = &[
        Parameter::logarithmic("cutoff", "Cutoff", 20.0, 20_000.0, 1_000.0).with_id(7),
        Parameter::bypass("bypass", "Bypass"),
        Parameter::linear("gain_db", "Gain", -60.0, 12.0, 0.0),
    ];
    let reordered = effect(REORDERED).resolve().unwrap();
    assert_eq!(
        reordered.parameter_ids(),
        [
            ParameterId(7),
            ParameterId(0x5dcb_1cad),
            ParameterId(0x046c_1bf5)
        ]
    );
}

#[test]
fn identities_match_foundry_derivations() {
    // Vectors computed independently from Foundry's SHA-256 derivations.
    let product = GAIN.resolve().unwrap();
    let ids = product.identities();
    assert_eq!(ids.clap_id, "com.example.gain");
    assert_eq!(ids.lv2_uri, "urn:com.example.gain");
    assert_eq!(
        ids.vst3_processor,
        [0x25ef_48ee, 0x8c09_430e, 0x38d8_90fe, 0xfdd4_ad17]
    );
    assert_eq!(
        ids.vst3_controller,
        [0x0838_3a51, 0x0290_47da, 0x0a61_183a, 0xd37f_f57c]
    );
    assert_eq!(&ids.au_type, b"aufx");
    assert_eq!(&ids.au_subtype, b"ZDHC");
    assert_eq!(&ids.au_manufacturer, b"HFKC");
    assert_eq!(ids.aax_manufacturer, 0xc7cc_78bf);
    assert_eq!(ids.aax_product, 0x0d47_b10d);
    assert_eq!(
        ids.aax_types,
        [
            (AudioLayout::STEREO, 0x0154_4488),
            (AudioLayout::MONO, 0x5f0e_29bc)
        ]
    );
}

#[test]
fn overrides_keep_earlier_identities() {
    let product = GAIN
        .with_identities(IdentityOverrides {
            clap_id: Some("com.legacy.gain"),
            vst3_classes: Some(([1, 2, 3, 4], [5, 6, 7, 8])),
            au_subtype: Some(*b"Lgcy"),
            au_manufacturer: Some(*b"Lgco"),
            aax_manufacturer: Some(11),
            aax_product: Some(12),
            lv2_uri: Some("https://example.com/plugins/gain"),
        })
        .resolve()
        .unwrap();
    let ids = product.identities();
    assert_eq!(ids.clap_id, "com.legacy.gain");
    assert_eq!(ids.vst3_processor, [1, 2, 3, 4]);
    assert_eq!(&ids.au_subtype, b"Lgcy");
    assert_eq!((ids.aax_manufacturer, ids.aax_product), (11, 12));
    assert_eq!(ids.lv2_uri, "https://example.com/plugins/gain");

    let bad = |overrides| GAIN.with_identities(overrides).resolve().unwrap_err();
    assert_eq!(
        bad(IdentityOverrides {
            au_subtype: Some(*b" abc"),
            ..IdentityOverrides::NONE
        }),
        ProductError::InvalidIdentity("au_subtype")
    );
    assert_eq!(
        bad(IdentityOverrides {
            lv2_uri: Some("no scheme"),
            ..IdentityOverrides::NONE
        }),
        ProductError::InvalidIdentity("lv2_uri")
    );
    assert_eq!(
        bad(IdentityOverrides {
            vst3_classes: Some(([1; 4], [1; 4])),
            ..IdentityOverrides::NONE
        }),
        ProductError::InvalidIdentity("vst3_classes")
    );
}

#[test]
fn instrument_defaults_to_stereo_output_and_note_input() {
    let synth = Product::new(
        "Synth",
        "Example",
        "0.1.0",
        "com.example.synth",
        Category::Instrument,
        FORMAT,
    )
    .resolve()
    .unwrap();
    assert_eq!(synth.product().layouts(), [AudioLayout::STEREO_INSTRUMENT]);
    assert_eq!(
        synth.product().notes(),
        NotePorts {
            input: true,
            output: false
        }
    );
    assert_eq!(&synth.identities().au_type, b"aumu");
}

#[test]
fn invalid_descriptions_are_rejected() {
    let error = |product: Product| product.resolve().unwrap_err();
    assert_eq!(error(GAIN.with_layouts(&[])), ProductError::NoLayouts);
    assert_eq!(
        error(GAIN.with_layouts(&[AudioLayout::STEREO_INSTRUMENT])),
        ProductError::EffectWithoutInput
    );
    assert_eq!(
        error(GAIN.with_layouts(&[AudioLayout::MONO, AudioLayout::MONO])),
        ProductError::DuplicateLayout(AudioLayout::MONO)
    );
    for version in ["1.0", "1.0.0.0", "01.0.0", "1.x.0", ""] {
        assert_eq!(
            error(Product::new(
                "A",
                "B",
                version,
                "com.example.a",
                Category::Effect,
                FORMAT
            )),
            ProductError::InvalidVersion
        );
    }
    for id in ["example", "com..example", "com.exa mple", ""] {
        assert_eq!(
            error(Product::new(
                "A",
                "B",
                "1.0.0",
                id,
                Category::Effect,
                FORMAT
            )),
            ProductError::InvalidPluginId
        );
    }
    assert_eq!(
        error(Product::new(
            " ",
            "B",
            "1.0.0",
            "com.example.a",
            Category::Effect,
            FORMAT
        )),
        ProductError::InvalidText("name")
    );
    assert_eq!(
        error(Product::new(
            "A",
            "B",
            "1.0.0",
            "com.example.a",
            Category::Effect,
            StateFormat {
                tag: *b"EXGAIN01",
                version: 0
            }
        )),
        ProductError::InvalidStateFormat
    );

    const BAD_KEY: &[Parameter] = &[Parameter::linear("2gain", "Gain", 0.0, 1.0, 0.0)];
    assert_eq!(error(effect(BAD_KEY)), ProductError::InvalidKey(0));
    const DUPLICATE_KEY: &[Parameter] = &[
        Parameter::linear("gain", "Gain", 0.0, 1.0, 0.0),
        Parameter::linear("gain", "Gain", 0.0, 1.0, 0.0).with_id(3),
    ];
    assert_eq!(error(effect(DUPLICATE_KEY)), ProductError::DuplicateKey(1));
    const DUPLICATE_ID: &[Parameter] = &[
        Parameter::linear("a", "A", 0.0, 1.0, 0.0).with_id(3),
        Parameter::linear("b", "B", 0.0, 1.0, 0.0).with_id(3),
    ];
    assert_eq!(
        error(effect(DUPLICATE_ID)),
        ProductError::DuplicateId {
            first: 0,
            second: 1
        }
    );
    const RESERVED_ID: &[Parameter] =
        &[Parameter::linear("a", "A", 0.0, 1.0, 0.0).with_id(u32::MAX)];
    assert_eq!(error(effect(RESERVED_ID)), ProductError::InvalidId(0));
    const TWO_BYPASSES: &[Parameter] = &[Parameter::bypass("a", "A"), Parameter::bypass("b", "B")];
    assert_eq!(error(effect(TWO_BYPASSES)), ProductError::InvalidBypass(1));
    const OFF_STEP_DEFAULT: &[Parameter] = &[Parameter::stepped("mode", "Mode", 0.0, 3.0, 3, 0.5)];
    assert_eq!(
        error(effect(OFF_STEP_DEFAULT)),
        ProductError::InvalidDefault(0)
    );
    const EMPTY_NAME: &[Parameter] = &[Parameter::linear("a", "", 0.0, 1.0, 0.0)];
    assert_eq!(
        error(effect(EMPTY_NAME)),
        ProductError::InvalidParameterText(0)
    );

    let keys: Vec<String> = (0..1025).map(|i| format!("p{i}")).collect();
    let many: Vec<Parameter> = keys
        .iter()
        .map(|key| Parameter::linear(key, "P", 0.0, 1.0, 0.0))
        .collect();
    assert_eq!(
        GAIN.with_parameters(&many).resolve().unwrap_err(),
        ProductError::TooManyParameters
    );
    assert!(GAIN.with_parameters(&many[..1024]).resolve().is_ok());
}

#[test]
fn editor_assets_are_validated() {
    use ravet_plugin::{Editor, EditorAsset, MAXIMUM_EDITOR_ASSET_BYTES};
    const EDITOR: Editor = Editor {
        html: "<p>x</p>",
        width: 300,
        height: 200,
        resizable: false,
    };
    const ASSETS: &[EditorAsset] = &[
        EditorAsset::new("app.js", b"ok()"),
        EditorAsset::new("img/knob.png", &[0x89, b'P', b'N', b'G']),
        EditorAsset {
            path: "data/curve.bin",
            mime: "application/x-curve",
            data: &[1, 2, 3],
        },
    ];
    let product = GAIN.with_editor(EDITOR).with_editor_assets(ASSETS);
    assert!(product.resolve().is_ok());
    assert_eq!(product.editor_assets().len(), 3);
    // Assets need an editor.
    assert_eq!(
        GAIN.with_editor_assets(ASSETS).resolve().err(),
        Some(ProductError::InvalidEditorAsset(0))
    );
    for (index, path) in [
        "",
        "/a",
        "a/",
        "a//b",
        "../x",
        "a/./b",
        "index.html",
        "sp ace",
        "q?x",
        "é",
    ]
    .into_iter()
    .enumerate()
    {
        let bad = [EditorAsset::new("ok.css", b""), EditorAsset::new(path, b"")];
        assert_eq!(
            GAIN.with_editor(EDITOR)
                .with_editor_assets(&bad)
                .resolve()
                .err(),
            Some(ProductError::InvalidEditorAsset(1)),
            "{index}: {path:?}"
        );
    }
    let duplicate = [EditorAsset::new("a.js", b""), EditorAsset::new("a.js", b"")];
    assert_eq!(
        GAIN.with_editor(EDITOR)
            .with_editor_assets(&duplicate)
            .resolve()
            .err(),
        Some(ProductError::InvalidEditorAsset(1))
    );
    let control = [EditorAsset {
        path: "a",
        mime: "text/\n",
        data: b"",
    }];
    assert_eq!(
        GAIN.with_editor(EDITOR)
            .with_editor_assets(&control)
            .resolve()
            .err(),
        Some(ProductError::InvalidEditorAsset(0))
    );
    let big = vec![0_u8; MAXIMUM_EDITOR_ASSET_BYTES + 1];
    let oversized = [EditorAsset::new("big.bin", &big)];
    assert_eq!(
        GAIN.with_editor(EDITOR)
            .with_editor_assets(&oversized)
            .resolve()
            .err(),
        Some(ProductError::InvalidEditorAsset(0))
    );
}
