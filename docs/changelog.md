# Changelog

<!-- markdownlint-configure-file { "MD004": false, "MD012": false, "MD024": { "siblings_only": true } } -->

Release-please writes each release's section from the conventional commit subjects that reach `main`.
Do not edit a generated section by hand. See [RELEASE.md](RELEASE.md#changelog).

## [0.1.2](https://github.com/mc-innersource/motif-mcao/compare/v0.1.1...v0.1.2) (2026-11-17)


### Features

* **core:** DPES-1105 - Add per-project delivery modes and a per-project task envelope ([c158f19](https://github.com/mc-innersource/motif-mcao/commit/c158f1980f7263cfbd799d1dbab21df3e225422d))
* **core:** DPES-1105 - Add the machine load gate and a start gate for the scheduler ([9a7bf56](https://github.com/mc-innersource/motif-mcao/commit/9a7bf563d55eab582bc7de6f6371f03d4d10a6eb))
* **core:** DPES-1105 - Let a caller choose each action's envelope refusal ([e0dcfb9](https://github.com/mc-innersource/motif-mcao/commit/e0dcfb92844144226ed2c49896a2bb6bacd31d98))
* **core:** DPES-1105 - Parse task proposals and check them against a fixed chat envelope ([733ab78](https://github.com/mc-innersource/motif-mcao/commit/733ab781a8c6625cfcda36a69d9d0ef9448725c4))
* **core:** DPES-1105 - Parse task.revise, project.remove and decision.hold, and name what can never be granted ([58521b8](https://github.com/mc-innersource/motif-mcao/commit/58521b86fe1c22040b30009249c555b4fc5d0a8e))
* **daemon:** DPES-1105 - Add the wake service, the stall check and monitoring ([cd83f29](https://github.com/mc-innersource/motif-mcao/commit/cd83f297f70d4fd0484a29c36ac8251ef4623938))
* **daemon:** DPES-1105 - Approve and run orchestrator action proposals ([c10d2e6](https://github.com/mc-innersource/motif-mcao/commit/c10d2e689cd3d1c85a1f1bfe76f7ff5b301f59be))
* **daemon:** DPES-1105 - Approve and run orchestrator action proposals ([32b445e](https://github.com/mc-innersource/motif-mcao/commit/32b445ed82f8b4519a154c9ce51b9511ec8772da))
* **daemon:** DPES-1105 - Capture scout reports, relay one on the user's request and promote a scout ([e2f46ae](https://github.com/mc-innersource/motif-mcao/commit/e2f46aec600eddd5b7afe9cbf9b94393de4d221f))
* **daemon:** DPES-1105 - Execute approved revise, hold and remove cards ([c15105f](https://github.com/mc-innersource/motif-mcao/commit/c15105fad6bfe32ee10f525f08d80ba8ef79bb6c))
* **daemon:** DPES-1105 - Execute approved task proposals and check them against a fixed envelope ([86ef929](https://github.com/mc-innersource/motif-mcao/commit/86ef929cc10ef4e34d3bbb4d98bc812a1dc2f90e))
* **daemon:** DPES-1105 - Gate task starts on machine load and add the settings methods ([cf1e430](https://github.com/mc-innersource/motif-mcao/commit/cf1e430c534a8e85b8b3a16bb0ab0675584dbf2d))
* **daemon:** DPES-1105 - Serve and enforce per-project delivery defaults ([5ae2f78](https://github.com/mc-innersource/motif-mcao/commit/5ae2f78a37330b0777b40bc1d7bc6268e5df332f))
* **daemon:** DPES-1105 - Start the daemon with an approver secret read from stdin ([4276dcc](https://github.com/mc-innersource/motif-mcao/commit/4276dccbb8c66964c3feb493e77d43ae38bd9313))
* **daemon:** DPES-1105 - Store action proposals parsed from orchestrator replies ([1dd0466](https://github.com/mc-innersource/motif-mcao/commit/1dd04663d087f5804cef8cbd46433c70f9b59ad1))
* **daemon:** DPES-1105 - Store action proposals parsed from orchestrator replies ([76ffb9d](https://github.com/mc-innersource/motif-mcao/commit/76ffb9d48a9198bdf2cdb582f3d712a0a9f52df7))
* **daemon:** DPES-1105 - Tell the orchestrator what the app is ([b2146f2](https://github.com/mc-innersource/motif-mcao/commit/b2146f2dd6c86e7da055dba527be5597c6be3e0b))
* **daemon:** DPES-1105 - Tell the orchestrator what the app is ([20b50ba](https://github.com/mc-innersource/motif-mcao/commit/20b50ba3b5b0c77518f5fe900103123e9e76bc56))
* **desktop:** DPES-1105 - Add Away mode and the return brief panel ([d10f03c](https://github.com/mc-innersource/motif-mcao/commit/d10f03c91b5de0f407299f7192e69c44aa6f6ec6))
* **desktop:** DPES-1105 - Add project delivery defaults, a Held tab and cards for revise, hold and remove ([6618b3e](https://github.com/mc-innersource/motif-mcao/commit/6618b3e1dd1e1202238a74c9e5e8511eaa470893))
* **desktop:** DPES-1105 - Add standing approvals settings section and grant approval label ([9743065](https://github.com/mc-innersource/motif-mcao/commit/97430655a2459745d514d97efbeda3f0c8cd5244))
* **desktop:** DPES-1105 - Add the Settings page for the machine load limits ([9d42ff1](https://github.com/mc-innersource/motif-mcao/commit/9d42ff1c5c1c5421e0a71fb3a29ab734b666fef9))
* **desktop:** DPES-1105 - Show approval cards for orchestrator actions ([453c235](https://github.com/mc-innersource/motif-mcao/commit/453c235fb17798743fa975429dfe6700804863dd))
* **desktop:** DPES-1105 - Show approval cards for orchestrator actions ([c752afe](https://github.com/mc-innersource/motif-mcao/commit/c752afe9abf93bb14dab287b31961f1acd7484fd))
* **desktop:** DPES-1105 - Show monitoring, wake notices and the supervision settings ([bd7a4fb](https://github.com/mc-innersource/motif-mcao/commit/bd7a4fbb507e19c70fbf9128010b2a2e48f12b5d))
* **desktop:** DPES-1105 - Show task proposal cards, a proposed actions queue and the approver notice ([834ff8f](https://github.com/mc-innersource/motif-mcao/commit/834ff8f375be8120008a59740eb2a94de2627f69))
* **desktop:** DPES-1105 - Show watched pull requests and the merge card ([bbbe9a4](https://github.com/mc-innersource/motif-mcao/commit/bbbe9a499e1de2f8b166b9ea0fa29e4ae4e3bd93))
* **ipc:** DPES-1105 - Add the holds capability and route the project defaults methods ([058d84a](https://github.com/mc-innersource/motif-mcao/commit/058d84a4230df380a58fb3f45d3083bda757778c))
* **ipc:** DPES-1105 - Add the monitoring capability with a user-only monitoring.set ([16d61ee](https://github.com/mc-innersource/motif-mcao/commit/16d61ee31a88c206ed72f6a18fc572a0ce76e964))
* **ipc:** DPES-1105 - Require an approver secret for user-only methods ([2db138e](https://github.com/mc-innersource/motif-mcao/commit/2db138ef191eb1dcf749d3c97188324ecacea1b9))
* **orchestrator:** DPES-1105 - Add away mode and the deterministic return brief ([f6b7e87](https://github.com/mc-innersource/motif-mcao/commit/f6b7e875f8f1471af1c10604c6c3ff2a8409e33d))
* **orchestrator:** DPES-1105 - Add orchestrator notes and apply the notes, MCP and CI review fixes ([d736aae](https://github.com/mc-innersource/motif-mcao/commit/d736aaea4f8d6ebaef5387083598009e721d5c47))
* **orchestrator:** DPES-1105 - Add routed-in strip, MCP setting and the read-only MCP server ([d599fa9](https://github.com/mc-innersource/motif-mcao/commit/d599fa9c16fb2b09e9e6da3d39bc68ab24437f54))
* **orchestrator:** DPES-1105 - Add standing grant core, store and ipc; fix Phase 4 review findings ([09d5448](https://github.com/mc-innersource/motif-mcao/commit/09d5448398a912ac57afe18635a5c76a36439dd8))
* **orchestrator:** DPES-1105 - Add standing grant matcher, grants methods and one-step grant approval ([a5d291f](https://github.com/mc-innersource/motif-mcao/commit/a5d291f5a3d9a3a45211e8c88766f5db488cc9ae))
* **orchestrator:** DPES-1105 - Add the GitHub trait with a fake, pull request store and landing capability ([645ceb3](https://github.com/mc-innersource/motif-mcao/commit/645ceb322bf24210f2cd41011383b8760f3bee12))
* **orchestrator:** DPES-1105 - Add the unattended merge grant ([c3ad8f8](https://github.com/mc-innersource/motif-mcao/commit/c3ad8f8876c237b13e6b305de719ca0a245af517))
* **orchestrator:** DPES-1105 - Complete orchestrator actions phases 2 to 6 ([592e4bc](https://github.com/mc-innersource/motif-mcao/commit/592e4bc8b000f987dba40d61b5d5c70155487626))
* **orchestrator:** DPES-1105 - Route work by scope with a workstream task list and routed-in view ([96aafac](https://github.com/mc-innersource/motif-mcao/commit/96aafac94ea6e789f92e0a00e34102dfa1c3f218))
* **orchestrator:** DPES-1105 - Watch pull requests and merge one on approval, with a fake host only ([0739658](https://github.com/mc-innersource/motif-mcao/commit/073965826a8425c59f8b7561551ad27462835cdb))
* **orchestrator:** DPES-1105 - Wire the read-only MCP server into Restricted Claude chat turns ([b6fbf3f](https://github.com/mc-innersource/motif-mcao/commit/b6fbf3f7e436b516734482586078f760a1e59d18))
* **store:** DPES-1105 - Add the Phase 4 action kinds, approve-time denial and project defaults ([a8811d6](https://github.com/mc-innersource/motif-mcao/commit/a8811d62f3c1bf79abe585ce9ef74f96162f0533))
* **store:** DPES-1105 - Add the settings row for machine load limits and a metrics observer ([f89d85d](https://github.com/mc-innersource/motif-mcao/commit/f89d85d8ad8ca63225ec8fd53e4702d85c6ba1ee))
* **store:** DPES-1105 - Store chat usage receipts, wake records and the supervision rules ([5dca750](https://github.com/mc-innersource/motif-mcao/commit/5dca75015f4dd0fe3886fe0622f1d152bd0044e4))
* **store:** DPES-1105 - Store held decisions, task lineage and a soft project archive ([e934115](https://github.com/mc-innersource/motif-mcao/commit/e934115fcf34ebb9adbac617c6e672f914058648))
* **store:** DPES-1105 - Store scout reports ([1d12ef5](https://github.com/mc-innersource/motif-mcao/commit/1d12ef51dd2d13674fec6ccaac38ed6f79fb26ba))
* **store:** DPES-1105 - Store task proposals and the reason an envelope refused one ([ff10629](https://github.com/mc-innersource/motif-mcao/commit/ff106292381ada3319f3779923290eec3df4f39a))


### Bug Fixes

* **core:** DPES-1105 - Apply the Phase 2 review fixes to proposals, the load gate and the metrics hook ([8654fde](https://github.com/mc-innersource/motif-mcao/commit/8654fdedd907bf1ac3358a4575bc1eef9fe4dd6f))
* **daemon:** DPES-1105 - Apply the Phase 3 review fixes to the wake service ([718bb2b](https://github.com/mc-innersource/motif-mcao/commit/718bb2b5ed50d312a6977b48a04653a1ddc697eb))
* **daemon:** DPES-1105 - Drain an over-long MCP line before closing, and update CI pins ([154b367](https://github.com/mc-innersource/motif-mcao/commit/154b367c1953eaf226d435646fc3be75f84b64e5))
* **daemon:** DPES-1105 - Keep project paths out of the snapshot ([7d2a361](https://github.com/mc-innersource/motif-mcao/commit/7d2a361f016171dc893324bb45da0c7292b2ae4c))
* **daemon:** DPES-1105 - Let only Restricted chat turns propose actions ([44776d2](https://github.com/mc-innersource/motif-mcao/commit/44776d2b49325a5f52a651082488752890732ce2))
* **daemon:** DPES-1106 - Run unverified agent versions with a warning and name what is unsupported ([814c5c9](https://github.com/mc-innersource/motif-mcao/commit/814c5c970316efa111cb3e1e6cc3d54cb214d225))
* **desktop:** DPES-1105 - Do not count held decisions in the nav, the top bar or the badge ([634614a](https://github.com/mc-innersource/motif-mcao/commit/634614a126e3eed659207efbf654f3d1c0114089))
* **desktop:** DPES-1105 - Keep the chosen provider and model across views ([49cf25e](https://github.com/mc-innersource/motif-mcao/commit/49cf25e3aa57e3a5a57a5190e2ab1b70116a19f2))
* **desktop:** DPES-1105 - Keep the proposals count honest and tell a non-approver why Request changes is off ([2375ea3](https://github.com/mc-innersource/motif-mcao/commit/2375ea33c8ed6d6912ea5eb958ae0a60c9183f71))
* **desktop:** DPES-1105 - Let the user clear a saved choice on a daemon that cannot take one ([c8991bf](https://github.com/mc-innersource/motif-mcao/commit/c8991bf4f1badd27727d694471945174a6d76417))
* **desktop:** DPES-1105 - Read the action card again after the daemon confirms it ([73586b6](https://github.com/mc-innersource/motif-mcao/commit/73586b6017c4b3e354cf84a7cfbdc5c86eefef03))
* **desktop:** DPES-1105 - Read the grant id the daemon sends on a proposal ([89cc1ef](https://github.com/mc-innersource/motif-mcao/commit/89cc1efc776915437f223abed54aa272ac20cc5c))
* **desktop:** DPES-1105 - Switch the action e2e test to Restricted before it sends ([8c7aabd](https://github.com/mc-innersource/motif-mcao/commit/8c7aabdaf55d93e46aafeb859776b514fc528fbf))
* **desktop:** DPES-1105 - Wait for the sign-in answer in the ModelPicker saved-choice test ([c0c4701](https://github.com/mc-innersource/motif-mcao/commit/c0c47013c741467c3efb099dd7e8e39951bf4bb0))
* **docs:** DPES-1106 - Let the generated changelog pass markdownlint ([80e7e93](https://github.com/mc-innersource/motif-mcao/commit/80e7e93a6745bb6a55b342344aaa3e0611830af3))
* **docs:** DPES-1106 - Let the generated changelog pass markdownlint ([e858d75](https://github.com/mc-innersource/motif-mcao/commit/e858d75492f913aa6486e19b466b2f101d0622e8))
* **git:** DPES-1105 - Refuse fork pull requests, qualify the host and tighten the remote parser ([1c7bdbf](https://github.com/mc-innersource/motif-mcao/commit/1c7bdbf261331243857ec88250ab3700a6834e79))
* **orchestrator:** DPES-1105 - Apply the pull request and merge grant review fixes ([1f3e813](https://github.com/mc-innersource/motif-mcao/commit/1f3e8133ef3f6330c9bfcb94fd62bb19d177640f))
* **orchestrator:** DPES-1105 - Let Auto chat turns propose actions ([64df1cd](https://github.com/mc-innersource/motif-mcao/commit/64df1cd0d56e555aab9ff7c564525756d04c48e2))
* **orchestrator:** DPES-1105 - Let Auto chat turns propose actions ([34d1b4f](https://github.com/mc-innersource/motif-mcao/commit/34d1b4fe3699913401fa3827e6a18e426731cc07))
* **orchestrator:** DPES-1105 - Stop a failing grant try from starving newer proposals ([467a3fc](https://github.com/mc-innersource/motif-mcao/commit/467a3fcfb95bc8c4a2fec59406867a9da52ff43f))
* **orchestrator:** DPES-1105 - Store a merge card from an Auto turn already denied ([02118a6](https://github.com/mc-innersource/motif-mcao/commit/02118a6c4e0d0344c88857c47df0ebfc6b1828f1))
* **store:** DPES-1105 - Simplify the pull request number check ([9585a98](https://github.com/mc-innersource/motif-mcao/commit/9585a988a7f95ca0eae43e8d5c39f1b5f4591b6b))


### Documentation

* **conversations:** DPES-1105 - Resolve the Restricted-only rebase conflict ([569969c](https://github.com/mc-innersource/motif-mcao/commit/569969cc83c4b487be6df7c1f33033264ad5f81d))
* **design:** DPES-1105 - Add supervision checks and Settings page (N10) ([b22f17b](https://github.com/mc-innersource/motif-mcao/commit/b22f17bbaa17de635a0592062987bf7ef5d974f0))
* **design:** DPES-1105 - Add the remote crew host threat model for Phase 6c ([42c088e](https://github.com/mc-innersource/motif-mcao/commit/42c088e72c3b1653f30b5c7fb8736c998cb3be81))
* **design:** DPES-1105 - Add workload backoff for wake checks (N11) ([d1258a8](https://github.com/mc-innersource/motif-mcao/commit/d1258a8008c826d0ebf0eb57228e0763e1a55d08))
* **design:** DPES-1105 - Drop N5 and N6 and renumber decisions ([27e886b](https://github.com/mc-innersource/motif-mcao/commit/27e886bb543f4c4230813e1ab4f3be07b64ecb12))
* **design:** DPES-1105 - Drop quota-aware dispatch from phase 6 ([996263f](https://github.com/mc-innersource/motif-mcao/commit/996263fe8e756b768a62bac8844d15ba8b2f0974))
* **design:** DPES-1105 - Drop quota-aware dispatch from phase 6 ([d5d4d27](https://github.com/mc-innersource/motif-mcao/commit/d5d4d274b7806e3c322f3682254d27e48adfc177))
* **design:** DPES-1105 - Gate dispatch on machine load ([a053406](https://github.com/mc-innersource/motif-mcao/commit/a05340606744df6174365b3136215b4641dc0d8d))
* **design:** DPES-1105 - Gate dispatch on machine load (N12) ([446dfaa](https://github.com/mc-innersource/motif-mcao/commit/446dfaa142c4cd77ea8ae50c4bb3a585c165560e))
* **design:** DPES-1105 - Record user decisions in the orchestrator actions design ([9c51bb7](https://github.com/mc-innersource/motif-mcao/commit/9c51bb787a4ce3df767f83123464e2e15a887b4d))
* **design:** DPES-1105 - Record user decisions N1 to N9 ([9c19261](https://github.com/mc-innersource/motif-mcao/commit/9c192618c6951bfc888f1240c24e4c06a7f0d082))
* **design:** DPES-1105 - Shorten wake interval as work grows ([0712300](https://github.com/mc-innersource/motif-mcao/commit/07123006d0f17bf245fbdcca48ee0db2ce8e0b2d))
* **design:** DPES-1105 - Shorten wake interval as work grows (N11) ([3da1a61](https://github.com/mc-innersource/motif-mcao/commit/3da1a6197865f5058737f2e54ce7e6918faad825))
* **design:** DPES-1105 - Trim firstmate map after user decisions ([e0a1bd3](https://github.com/mc-innersource/motif-mcao/commit/e0a1bd3186a042bd3b067a1845ccf7a5b6e06107))
* **orchestrator:** DPES-1105 - Document phases 4 to 6, standing approvals and residual risks ([4bf9595](https://github.com/mc-innersource/motif-mcao/commit/4bf9595efe00d7f1dcccb2933888477c156a9849))
* **orchestrator:** DPES-1105 - Document pull request merge, the merge grant and the live checklist ([e72103e](https://github.com/mc-innersource/motif-mcao/commit/e72103e7a4ec331a2d73bc6e8f18dd932b74b2a1))


### Tests

* **daemon:** DPES-1105 - Bound the MCP port probes and say why the relay closed ([3fdb154](https://github.com/mc-innersource/motif-mcao/commit/3fdb1541f90ef09cd77406c0868c2f61ef992cea))
* **daemon:** DPES-1105 - Let a task finish before the wire tests stop the daemon ([7aed33d](https://github.com/mc-innersource/motif-mcao/commit/7aed33ded5aa7d419c3198845d1f5de7122fd3cc))
* **desktop:** DPES-1105 - Wait for the model picker label and record the Windows daemon names ([541da16](https://github.com/mc-innersource/motif-mcao/commit/541da16fbf5e23e64288e1ce456f361cd3887308))
* **desktop:** select Restricted before the real-daemon action-proposal e2e ([74bddd1](https://github.com/mc-innersource/motif-mcao/commit/74bddd118b77d810b98b5c82d89bb218945e11c3))
* **store:** DPES-1105 - Cover the over-cap block recorded as rejected in one transaction ([7e00909](https://github.com/mc-innersource/motif-mcao/commit/7e00909cff129529ef845d264375ac635e5012c8))


### Build

* **deps:** DPES-1105 - Record the git crate's new dependencies in the lockfile ([6bdd44f](https://github.com/mc-innersource/motif-mcao/commit/6bdd44f936343574eec5798c2ef0fe51fd189f0e))


### CI

* **gates:** DPES-1105 - Name the resolve-aware Restricted harness test ([06a513e](https://github.com/mc-innersource/motif-mcao/commit/06a513ea8448cb62d3aca9aa3c6f421d21ba1850))
* **gates:** DPES-1105 - Name the Restricted-only harness test in the gates ([df80b8e](https://github.com/mc-innersource/motif-mcao/commit/df80b8edcc53602a8e622e8a234f7467c61ee8c1))
* **gates:** DPES-1105 - Update expected counts and name lists for phases 2 to 6 ([2817a67](https://github.com/mc-innersource/motif-mcao/commit/2817a67956bcd399bee92ebd9e6b036a83ed1936))
* **gates:** DPES-1105 - Update expected counts and name lists from CI run 44023050296 ([1c29c44](https://github.com/mc-innersource/motif-mcao/commit/1c29c449971ba1f86d55ebd4f2e2cecb89e4b508))
* **gates:** DPES-1105 - Update expected counts and name lists from CI runs 43994386560 and 43998558420 ([66cc32a](https://github.com/mc-innersource/motif-mcao/commit/66cc32a80a4ba9e78b65c86e6d6b1ee47d3be2bf))
* **gates:** DPES-1105 - Update expected desktop test count from CI run 43963037995 ([06ef4e6](https://github.com/mc-innersource/motif-mcao/commit/06ef4e666746ab15571131687b08214758274fa3))
* **gates:** DPES-1105 - Update expected desktop test count from CI run 43974286338 ([02792d4](https://github.com/mc-innersource/motif-mcao/commit/02792d40416d484200561f1c41b178cfd475a1ea))
* **gates:** DPES-1105 - Update expected test counts from CI run 43974417288 ([edeec47](https://github.com/mc-innersource/motif-mcao/commit/edeec47740d31cf4958bb9ac2bbf80589bd55943))
* **gates:** DPES-1105 - Update expected test counts from CI run 43981399384 ([410ae5b](https://github.com/mc-innersource/motif-mcao/commit/410ae5be64ea17dd3f16401a4ac688c8ee5f52c1))
* **gates:** DPES-1106 - Count the release-please node tests as CI measured them ([a26efca](https://github.com/mc-innersource/motif-mcao/commit/a26efca542a087e2b7d9f074af3d25395d9b5eed))
* **gates:** DPES-1106 - Count the release-please node tests as CI measured them ([fd88b10](https://github.com/mc-innersource/motif-mcao/commit/fd88b107d5545cf324a864258493a11cccc71ffb))
* **gates:** DPES-1106 - Update expected test counts from CI run 43965574766 ([88318af](https://github.com/mc-innersource/motif-mcao/commit/88318af55be3b0cc191840672f0cfaa802606e06))
* **native:** DPES-1105 - Reconcile native CI pins and fix action-proposal e2e ([8fac29d](https://github.com/mc-innersource/motif-mcao/commit/8fac29dba1eb48416e5d92da44efdfcffeccbe36))
* update native test inventories to measured counts ([0c47906](https://github.com/mc-innersource/motif-mcao/commit/0c47906464e3671e451abf94acebff78792ae3e4))

## [0.1.1](https://github.com/mc-innersource/motif-mcao/compare/v0.1.0...v0.1.1) (2026-11-16)


### Features

* **desktop:** DPES-1105 - Let the model picker choose GitHub Copilot for chat ([89bfc8b](https://github.com/mc-innersource/motif-mcao/commit/89bfc8b2dc6baa9fc1d6f1fe1ff89a72a89bf617))
* **desktop:** DPES-1105 - Let the model picker choose GitHub Copilot for chat ([8b55696](https://github.com/mc-innersource/motif-mcao/commit/8b55696b449071b886fdd38b370811e607d3e326))


### Bug Fixes

* **desktop:** DPES-1105 - Keep not available for uncosted sessions, make the daemon test hermetic ([eada0d2](https://github.com/mc-innersource/motif-mcao/commit/eada0d2619c497855046c6a9aac3bf4ce028512c))
* **desktop:** DPES-1105 - Make the usage pill and the other tools opt-in work ([9587d24](https://github.com/mc-innersource/motif-mcao/commit/9587d24837b4173514b894a50206d3024929d26a))
* **desktop:** DPES-1105 - Make the usage pill and the other tools opt-in work ([ee54560](https://github.com/mc-innersource/motif-mcao/commit/ee54560a968cac2148d0f25796f643be63550bd9))


### Documentation

* **changelog:** DPES-1106 - Generate the changelog from commit subjects ([3c77f64](https://github.com/mc-innersource/motif-mcao/commit/3c77f645996f01d2045c244cfb84b47d5cb0c4c0))
* **design:** DPES-1105 - Design orchestrator actions and firstmate parity ([c37a8af](https://github.com/mc-innersource/motif-mcao/commit/c37a8af1b3d8d47426096dc67fae60e21d582bc1))
* **design:** DPES-1105 - Design orchestrator actions and firstmate parity ([692fbc6](https://github.com/mc-innersource/motif-mcao/commit/692fbc6311e5468c46640f0308f40bd592a5f459))
* **readme:** DPES-1106 - Show the unsigned install notice at the top ([ff144ea](https://github.com/mc-innersource/motif-mcao/commit/ff144ea819a0f6cff4f089c3c5d51d55ebc1448a))
* **readme:** DPES-1106 - Show the unsigned install notice at the top ([88f528e](https://github.com/mc-innersource/motif-mcao/commit/88f528ea07153bfb6db79f8615d65f1a19fb73f6))
* **release:** DPES-1106 - Describe the release-please release flow ([aeb52c0](https://github.com/mc-innersource/motif-mcao/commit/aeb52c04853b66ba44f7fdf0808d91b112e6657a))


### Tests

* **ci:** DPES-1105 - Count the usage controls tests as CI measured them ([518e74d](https://github.com/mc-innersource/motif-mcao/commit/518e74d0fe75ae9dcf7d92030b24ef80c3103b25))


### CI

* **gates:** DPES-1105 - Update expected test counts from CI run 43951278199 ([caaf9ed](https://github.com/mc-innersource/motif-mcao/commit/caaf9ed640bfcc6a0ccc51d7028e39182a9af758))
* **gates:** DPES-1106 - Read the installer version from the app config ([474b0e4](https://github.com/mc-innersource/motif-mcao/commit/474b0e4aec60f6277f8aa3a4b41a0574a1395776))
* **release:** DPES-1106 - Put the version's changelog section in the release notes ([77df643](https://github.com/mc-innersource/motif-mcao/commit/77df643e8bae8e85726509e27ed60e7803c16809))
* **workflows:** DPES-1106 - Open release pull requests with release-please ([fb3be4b](https://github.com/mc-innersource/motif-mcao/commit/fb3be4bb0db36e94a0415a0ef016daf190f52eb4))
* **workflows:** DPES-1106 - Open release pull requests with release-please ([3f54e9f](https://github.com/mc-innersource/motif-mcao/commit/3f54e9f914075d453d0255a5ac856326b9fe386f))

## v0.1.0

- Prevent the model selector from choosing or validating providers when none are launchable.
- Add a per-machine task branch prefix read from `<data_dir>/config/branch-prefix`; the default changes from `orchestrator/` to `moa/` (DPES-681). Existing `orchestrator/*` task branches stay valid for verification.
