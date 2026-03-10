window.BENCHMARK_DATA = {
  "lastUpdate": 1773168537572,
  "repoUrl": "https://github.com/jaredwolff/coapum",
  "entries": {
    "coapum Criterion": [
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "d0829adb090de88799a4e7b3af1a3fa69ec87940",
          "message": "fix: saving bench output to root on gh-pages",
          "timestamp": "2025-08-19T21:17:54-04:00",
          "tree_id": "c2e0bd07a2b66cb5ad08eddd99378d45c4b1cf5b",
          "url": "https://github.com/jaredwolff/coapum/commit/d0829adb090de88799a4e7b3af1a3fa69ec87940"
        },
        "date": 1755652793924,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1147,
            "range": "± 18",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "96c81a9073dd606cfc3cc56a4f82d7ac913eb2ed",
          "message": "ci: add workflow_dispatch trigger to performance workflow",
          "timestamp": "2026-03-07T13:51:23-05:00",
          "tree_id": "d2501f90b2743bd834bce35adbdcb7c2b2356e85",
          "url": "https://github.com/jaredwolff/coapum/commit/96c81a9073dd606cfc3cc56a4f82d7ac913eb2ed"
        },
        "date": 1772909617100,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1123,
            "range": "± 2",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "ee46ea86a0d88b7766413ef3755fbd98fa96e80e",
          "message": "fix(ci): use cargo bench stdout for benchmark-action\n\nThe benchmark-action tool: \"cargo\" expects captured stdout from\ncargo bench, not Criterion's estimates.json file.",
          "timestamp": "2026-03-07T13:55:28-05:00",
          "tree_id": "8de232382cfea9b0dff3a0db1092d116502f903a",
          "url": "https://github.com/jaredwolff/coapum/commit/ee46ea86a0d88b7766413ef3755fbd98fa96e80e"
        },
        "date": 1772909850981,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1056,
            "range": "± 4",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "4b79bd7a2fcc8a82eb51a8adfb4122421fa74902",
          "message": "fix(ci): use bencher output format for benchmark-action compatibility\n\nCriterion's default output isn't parsed by benchmark-action tool:\n\"cargo\". Use --output-format bencher to produce the expected format.\nDisable CARGO_TERM_COLOR to avoid ANSI codes in output file.",
          "timestamp": "2026-03-07T14:00:19-05:00",
          "tree_id": "b5410b3e22b491661d2892638edcfd8bdb17d0d1",
          "url": "https://github.com/jaredwolff/coapum/commit/4b79bd7a2fcc8a82eb51a8adfb4122421fa74902"
        },
        "date": 1772910144863,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1056,
            "range": "± 13",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "a769d35ffa8d4c6590177fec8810efd3717211e8",
          "message": "fix(ci): only capture stdout for benchmark output\n\nStderr contains compilation logs that pollute the benchmark output\nfile and prevent benchmark-action from parsing results.",
          "timestamp": "2026-03-07T14:00:47-05:00",
          "tree_id": "758e2a8b6c8edd566b6b0ab089b2bd19db22d6af",
          "url": "https://github.com/jaredwolff/coapum/commit/a769d35ffa8d4c6590177fec8810efd3717211e8"
        },
        "date": 1772910351343,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1051,
            "range": "± 34",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "36fba78b356fc48141a7a1965e900bf4e96a2222",
          "message": "fix(ci): fix benchmark workflow for benchmark-action compatibility\n\n- Use --output-format bencher for Criterion output (required by\n  benchmark-action tool: \"cargo\")\n- Only capture stdout to avoid compilation logs in output file\n- Add workflow_dispatch trigger for manual runs",
          "timestamp": "2026-03-07T14:01:19-05:00",
          "tree_id": "758e2a8b6c8edd566b6b0ab089b2bd19db22d6af",
          "url": "https://github.com/jaredwolff/coapum/commit/36fba78b356fc48141a7a1965e900bf4e96a2222"
        },
        "date": 1772910458647,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1053,
            "range": "± 3",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "ee56252b8a6573c1cedffcc57caa15ea263d1713",
          "message": "fix: bump MSRV to 1.89, fix time vulnerability, fix new clippy lints\n\n- Bump MSRV from 1.85.0 to 1.89.0 (required by redb 3.0.0)\n- Update time crate to 0.3.47 to fix RUSTSEC-2026-0009\n- Fix collapsible_if and is_multiple_of clippy lints from Rust 1.89+\n- Add contents: write permission for benchmark gh-pages push",
          "timestamp": "2026-03-07T14:07:25-05:00",
          "tree_id": "a014aeb69d29ac2e60da2ea76a140add9fefbec9",
          "url": "https://github.com/jaredwolff/coapum/commit/ee56252b8a6573c1cedffcc57caa15ea263d1713"
        },
        "date": 1772910668902,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1063,
            "range": "± 17",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "f78fa365697e3a895f3ee7cddcebb699f7017b99",
          "message": "fix(ci): remove broken performance workflow placeholder jobs\n\nRemove memory-profiling, load-test, resource-usage, and\nperformance-summary jobs that all tried to run DTLS examples\nwithout PSK configuration, causing timeouts in CI. Keep only\nthe working benchmark job.",
          "timestamp": "2026-03-07T15:36:42-05:00",
          "tree_id": "799ae5d416168421230da62d5135ae276a8f13d1",
          "url": "https://github.com/jaredwolff/coapum/commit/f78fa365697e3a895f3ee7cddcebb699f7017b99"
        },
        "date": 1772915948362,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1088,
            "range": "± 22",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "5366b0898a90182ee63c5e0ec727ddcfb94a4b57",
          "message": "fix(ci): remove broken performance workflow placeholder jobs\n\nRemove memory-profiling, load-test, resource-usage, and\nperformance-summary jobs that all tried to run DTLS examples\nwithout PSK configuration, causing timeouts in CI. Keep only\nthe working benchmark job.",
          "timestamp": "2026-03-07T15:46:56-05:00",
          "tree_id": "6d5e995ab9c732a5f263ec686284959a270973cf",
          "url": "https://github.com/jaredwolff/coapum/commit/5366b0898a90182ee63c5e0ec727ddcfb94a4b57"
        },
        "date": 1772916488822,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1056,
            "range": "± 4",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "5366b0898a90182ee63c5e0ec727ddcfb94a4b57",
          "message": "fix(ci): remove broken performance workflow placeholder jobs\n\nRemove memory-profiling, load-test, resource-usage, and\nperformance-summary jobs that all tried to run DTLS examples\nwithout PSK configuration, causing timeouts in CI. Keep only\nthe working benchmark job.",
          "timestamp": "2026-03-07T15:46:56-05:00",
          "tree_id": "6d5e995ab9c732a5f263ec686284959a270973cf",
          "url": "https://github.com/jaredwolff/coapum/commit/5366b0898a90182ee63c5e0ec727ddcfb94a4b57"
        },
        "date": 1772916569995,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1093,
            "range": "± 15",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "8505a0dc8c4a296314eccc3408405223429341b5",
          "message": "deps: use git dependency for dimpl instead of local path\n\nFixes CI builds by pointing to the circuitdojo/dimpl fork\ninstead of a sibling directory that doesn't exist in CI.",
          "timestamp": "2026-03-09T15:04:11-04:00",
          "tree_id": "181ce7abb2b4c773443ca11a6c84838af3b471ac",
          "url": "https://github.com/jaredwolff/coapum/commit/8505a0dc8c4a296314eccc3408405223429341b5"
        },
        "date": 1773083212665,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1042,
            "range": "± 4",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "8505a0dc8c4a296314eccc3408405223429341b5",
          "message": "deps: use git dependency for dimpl instead of local path\n\nFixes CI builds by pointing to the circuitdojo/dimpl fork\ninstead of a sibling directory that doesn't exist in CI.",
          "timestamp": "2026-03-09T15:04:11-04:00",
          "tree_id": "181ce7abb2b4c773443ca11a6c84838af3b471ac",
          "url": "https://github.com/jaredwolff/coapum/commit/8505a0dc8c4a296314eccc3408405223429341b5"
        },
        "date": 1773083296804,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1034,
            "range": "± 9",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "bd119df167a3b50b738208a478b9f833b99e6da6",
          "message": "feat: add extract_wildcard_path for multi-segment path extraction\n\nPath<String> now uses extract_wildcard_path instead of\nextract_wildcard_param, preserving the full hierarchical path after the\nroute prefix (e.g. t/humidity/test -> humidity/test). The original\nextract_wildcard_param (last-segment only) is kept and re-exported for\ncallers that need it.",
          "timestamp": "2026-03-09T18:27:17-04:00",
          "tree_id": "6eff00874c8d717ed74e13449ea2493bcd0d5325",
          "url": "https://github.com/jaredwolff/coapum/commit/bd119df167a3b50b738208a478b9f833b99e6da6"
        },
        "date": 1773095302323,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1081,
            "range": "± 30",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "bd119df167a3b50b738208a478b9f833b99e6da6",
          "message": "feat: add extract_wildcard_path for multi-segment path extraction\n\nPath<String> now uses extract_wildcard_path instead of\nextract_wildcard_param, preserving the full hierarchical path after the\nroute prefix (e.g. t/humidity/test -> humidity/test). The original\nextract_wildcard_param (last-segment only) is kept and re-exported for\ncallers that need it.",
          "timestamp": "2026-03-09T18:27:17-04:00",
          "tree_id": "6eff00874c8d717ed74e13449ea2493bcd0d5325",
          "url": "https://github.com/jaredwolff/coapum/commit/bd119df167a3b50b738208a478b9f833b99e6da6"
        },
        "date": 1773095415640,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1039,
            "range": "± 5",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "b273f3e011ee671146614180b645d2135d50cd4c",
          "message": "fix: forward trigger_notification value in observe response payload\n\nThe notification handler called router.call() but never placed the\nactual ObserverValue data into the response payload. Clone the value\nbefore to_request() consumes it, then serialize it into the response\nusing CBOR or JSON based on the handler's content format.",
          "timestamp": "2026-03-10T01:15:01-04:00",
          "tree_id": "305530f462a27c6cb517faf56f9e67a799f2c552",
          "url": "https://github.com/jaredwolff/coapum/commit/b273f3e011ee671146614180b645d2135d50cd4c"
        },
        "date": 1773119832484,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1060,
            "range": "± 4",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "b273f3e011ee671146614180b645d2135d50cd4c",
          "message": "fix: forward trigger_notification value in observe response payload\n\nThe notification handler called router.call() but never placed the\nactual ObserverValue data into the response payload. Clone the value\nbefore to_request() consumes it, then serialize it into the response\nusing CBOR or JSON based on the handler's content format.",
          "timestamp": "2026-03-10T01:15:01-04:00",
          "tree_id": "305530f462a27c6cb517faf56f9e67a799f2c552",
          "url": "https://github.com/jaredwolff/coapum/commit/b273f3e011ee671146614180b645d2135d50cd4c"
        },
        "date": 1773119914662,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1038,
            "range": "± 18",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "90541e517bf84ce9ab9b0dd8db02a850d7eb0ecc",
          "message": "security: harden identity validation, CBOR depth limits, and credential docs\n\n- Reject PSK identities with invalid characters instead of silently\n  stripping them, preventing identity collisions (e.g. \"dev@1\" → \"dev.1\")\n- Add CBOR recursion depth limit (32) via from_reader_with_recursion_limit\n  to prevent stack overflow from deeply nested payloads\n- Expand CredentialStore::lookup_psk docs with concrete anti-patterns and\n  recommended sync access patterns\n- Document PskEntry key zeroization limitation (dimpl doesn't zeroize either)\n- Document Bytes/Raw extractor transport-layer size bounds",
          "timestamp": "2026-03-10T12:50:18-04:00",
          "tree_id": "8f8936c0b9244fb801be9f4d22aa3bf3aeba2d57",
          "url": "https://github.com/jaredwolff/coapum/commit/90541e517bf84ce9ab9b0dd8db02a850d7eb0ecc"
        },
        "date": 1773161501594,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1116,
            "range": "± 11",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "90541e517bf84ce9ab9b0dd8db02a850d7eb0ecc",
          "message": "security: harden identity validation, CBOR depth limits, and credential docs\n\n- Reject PSK identities with invalid characters instead of silently\n  stripping them, preventing identity collisions (e.g. \"dev@1\" → \"dev.1\")\n- Add CBOR recursion depth limit (32) via from_reader_with_recursion_limit\n  to prevent stack overflow from deeply nested payloads\n- Expand CredentialStore::lookup_psk docs with concrete anti-patterns and\n  recommended sync access patterns\n- Document PskEntry key zeroization limitation (dimpl doesn't zeroize either)\n- Document Bytes/Raw extractor transport-layer size bounds",
          "timestamp": "2026-03-10T12:50:18-04:00",
          "tree_id": "8f8936c0b9244fb801be9f4d22aa3bf3aeba2d57",
          "url": "https://github.com/jaredwolff/coapum/commit/90541e517bf84ce9ab9b0dd8db02a850d7eb0ecc"
        },
        "date": 1773161561530,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1055,
            "range": "± 7",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "ddc28e67c59dde4b3c4f4e89e9d3cb0837b90949",
          "message": "feat: add cargo-fuzz targets for security-critical input handling\n\nFive fuzz targets covering the main attack surface:\n- coap_parse: pre-auth CoAP packet parsing\n- cbor_deser: CBOR deserialization with depth/size limits\n- json_deser: JSON deserialization with size limits\n- identity_validate: DTLS identity validation with invariant checks\n- observer_path: observer path validation with traversal assertions\n\nExposes extract_identity via test-utils feature for fuzz testing.",
          "timestamp": "2026-03-10T13:48:38-04:00",
          "tree_id": "45b61c8b494236206d2dc167eb597be2fa6afc94",
          "url": "https://github.com/jaredwolff/coapum/commit/ddc28e67c59dde4b3c4f4e89e9d3cb0837b90949"
        },
        "date": 1773168473995,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1061,
            "range": "± 22",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "committer": {
            "email": "hello@jaredwolff.com",
            "name": "Jared Wolff",
            "username": "jaredwolff"
          },
          "distinct": true,
          "id": "ddc28e67c59dde4b3c4f4e89e9d3cb0837b90949",
          "message": "feat: add cargo-fuzz targets for security-critical input handling\n\nFive fuzz targets covering the main attack surface:\n- coap_parse: pre-auth CoAP packet parsing\n- cbor_deser: CBOR deserialization with depth/size limits\n- json_deser: JSON deserialization with size limits\n- identity_validate: DTLS identity validation with invariant checks\n- observer_path: observer path validation with traversal assertions\n\nExposes extract_identity via test-utils feature for fuzz testing.",
          "timestamp": "2026-03-10T13:48:38-04:00",
          "tree_id": "45b61c8b494236206d2dc167eb597be2fa6afc94",
          "url": "https://github.com/jaredwolff/coapum/commit/ddc28e67c59dde4b3c4f4e89e9d3cb0837b90949"
        },
        "date": 1773168536725,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1120,
            "range": "± 13",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}