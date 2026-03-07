window.BENCHMARK_DATA = {
  "lastUpdate": 1772915949346,
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
      }
    ],
    "Rust Benchmark": [
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
        "date": 1772910558567,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1145,
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
          "id": "f78fa365697e3a895f3ee7cddcebb699f7017b99",
          "message": "fix(ci): remove broken performance workflow placeholder jobs\n\nRemove memory-profiling, load-test, resource-usage, and\nperformance-summary jobs that all tried to run DTLS examples\nwithout PSK configuration, causing timeouts in CI. Keep only\nthe working benchmark job.",
          "timestamp": "2026-03-07T15:36:42-05:00",
          "tree_id": "799ae5d416168421230da62d5135ae276a8f13d1",
          "url": "https://github.com/jaredwolff/coapum/commit/f78fa365697e3a895f3ee7cddcebb699f7017b99"
        },
        "date": 1772915861563,
        "tool": "cargo",
        "benches": [
          {
            "name": "coap_router",
            "value": 1097,
            "range": "± 10",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}