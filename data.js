window.BENCHMARK_DATA = {
  "lastUpdate": 1772909851306,
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
      }
    ]
  }
}