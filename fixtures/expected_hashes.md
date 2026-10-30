# Expected output hashes (seed 1)

`full` values are the published ones from the organizer specs. `tiny` and `sample` were generated and run with the
organizers' own generators and Python references (fixtures/gen_data.sh + run_ref.sh) and the `sample` values match
the spec tables. Refill the "your machine" column when you run them yourself; a difference means your setup is wrong.

| Track | Output file | tiny | sample | full (spec) |
|---|---|---|---|---|
| A | summary.txt | c24fa507c963eaeac9224fd8435019b4f4e8c002ece7e32f316681a3957eb92f | ca6311bbeb536fa55d85265bf6fd3d24cc9586f13de1ac73bd2a23ac3893aac3 | 495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3 |
| B | results.txt | dcdad7cedfc07eb66ecced1861cc91d9017e004c56cbe4f38e9413b62b17ff70 | eeeec1e9319da98a6f3c664c78f93e7078fd603c25c2bb3027174e81e3638a8f | f86973c210228141bcb6f1f1b10188ad2a9b65f71e7cd7cf4590b38de4e80927 |
| C | aggregates.txt | e4e43bc39920e9d446c3fcd700fdd35b3959e9495c6aaa738491df38a1aed733 | 9035fed36823232dd41ae2d1b9d6d21aa7c73f49d8e95647e953a89d5ebe9a66 | d8088b1cd42ba1338b1345b107b55db7b96ef1f5066f3ff64fd26a8912a0f536 |
| D | distances.txt | ecaca77d3221fe3928109da36480cfdb39f288f2e57ecbc61d3d7706d5cb4bd5 | a4c76ed030bc7e365db33191f6cf74708fba7f25eb48d64c8c4279f2e57046a0 | a23dbdf0a3e49cc8901310a9ee1dc6a9a569a85297b6da75c2a7002cb2ed277e |

Input hashes (B, seed 1, from spec B): docs.txt sample e157fabb5874c6058a3f6c7726f51df08b2369664cc76c52dde62335ef57eaa4,
queries.txt sample 930ba8716d7a9b9b962b24f5c9d11b5e0b1efb1b9c24f775f889e50c639f16aa; docs.txt full
6cd461670d6c39f9c5211162577532d3648edeab12340c71cc7d2ab5f18e66c8, queries.txt full
77ef7ffc59afde051006b02a2ce8e4c090fbbfbf101acd6f862d0f8e298d39f9.

Hand-made fixtures (known answers taken from the spec text, not from our own reasoning):
- custom/b/spec_example/ : the 5-document, 18-query worked example of spec B; expected_results.txt is the spec's exact output
  (sha256 9c296c063de983fd734d9e6ab985866967731d6478d33b7e1b8723391b6e325b).
