window.BENCHMARK_DATA = {
  "lastUpdate": 1772910351665,
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
      }
    ]
  }
}