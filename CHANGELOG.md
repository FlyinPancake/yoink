# Changelog

## [0.1.4](https://github.com/FlyinPancake/yoink/compare/yoink-v0.1.3...yoink-v0.1.4) (2026-04-11)


### ⚠ BREAKING CHANGES

* **api:** API fields `provider` and `album_type` are now enum values (Provider, AlbumType) instead of plain strings. Clients must handle enum values and regenerate client types where applicable.
* **api:** The API enum value previously "unwanted" is now "unmonitored". Update clients and regenerate frontend types (run mise run gen-frontend-types) to match the new contract.
* **error:** public/shared error types and function signatures changed. AsyncFnResult and several server APIs now return YoinkError/AppError instead of String; consumers must update error handling to the new types.
* **providers:** MetadataProvider::fetch_artist_image_ref signature now accepts an additional Option<&str> name_hint parameter. Update provider implementations to the new signature.
* **providers:** database schema and public types changed. IDs are now UUID v7 (BLOB/strings) instead of integer Tidal IDs; the old yoink.db is incompatible — delete the existing DB files and start with a fresh database.

### Features

* a good amount of improvements ([6ea2396](https://github.com/FlyinPancake/yoink/commit/6ea23961906f90695c42225ab5366cf054985456))
* Add album and track download quality overrides ([abd3639](https://github.com/FlyinPancake/yoink/commit/abd3639c1277f32054efccba043c1eac10884654))
* add album card component and monitor toggle ([01da535](https://github.com/FlyinPancake/yoink/commit/01da535e8a9320e530b3383346994bf01a170008))
* add album match suggestions and monitoring updates ([b642541](https://github.com/FlyinPancake/yoink/commit/b642541bd385642b5c8c2629d192aafeeaaf6402))
* add button, page shell, breadcrumb ([b5ba1fe](https://github.com/FlyinPancake/yoink/commit/b5ba1fe2b47c5549493991942763de90a7c13e68))
* add confirmation dialogs for destructive actions ([5e4c187](https://github.com/FlyinPancake/yoink/commit/5e4c18775a6e613a40e792c009fc4921b2aa5254))
* add Deezer metadata provider (Phase 5) ([97f2c51](https://github.com/FlyinPancake/yoink/commit/97f2c51b56bd6a1b98fe66cc0b1e4cfc3cf38c20))
* add external import ([f7b0ff0](https://github.com/FlyinPancake/yoink/commit/f7b0ff008596547d40e7d7607df09f33df53668b))
* add Link Provider dialog for multi-provider artist linking ([e6e5d5e](https://github.com/FlyinPancake/yoink/commit/e6e5d5ec8f4d388c6d3db4881a59e4c1776e3467))
* add MusicBrainz provider, fix album dedup, enrich search results ([2862c8e](https://github.com/FlyinPancake/yoink/commit/2862c8e9c2b5606a0437fe31f751745908dda905))
* add SoulSeek download source and decouple download metadata resolution ([cb1b716](https://github.com/FlyinPancake/yoink/commit/cb1b716a67cda49d9c9ba75387f560bd924e1a1b))
* add toast notifications for all server actions ([a5da479](https://github.com/FlyinPancake/yoink/commit/a5da479cfba7a8f60ade56ce6aed51bd24a0cd4e))
* add track artist and file_path support ([6c7f96d](https://github.com/FlyinPancake/yoink/commit/6c7f96dc96144e0309b94089a7c1f896a2e07ab5))
* **album:** add album quality_override ([33fb05f](https://github.com/FlyinPancake/yoink/commit/33fb05fde7e3375c10db44457446dbbeadeade6e))
* **albums:** support multiple artists per album ([e14f1d2](https://github.com/FlyinPancake/yoink/commit/e14f1d2130c82ec9387b6af22537afc87358eecc))
* **api:** rename unwanted to unmonitored ([3c1188c](https://github.com/FlyinPancake/yoink/commit/3c1188cb9bd7da44afe74d380b225c0693ad7e84))
* artist bio, provider icons, and UI refinements ([67ff479](https://github.com/FlyinPancake/yoink/commit/67ff479c649bb5ebd91a9556e1731401476b33cf))
* **artist:** add artist image and bio actions ([166965d](https://github.com/FlyinPancake/yoink/commit/166965d9554077f6ffe7c610cdd76c7caf76aee2))
* **badge:** add inline badge component ([42c4756](https://github.com/FlyinPancake/yoink/commit/42c4756dabeacb20bb07633c81a30eee07dd3120))
* **components:** add sleeve badge component ([f13034c](https://github.com/FlyinPancake/yoink/commit/f13034cc7302db1dac31f503fb63763f00bd9ec7))
* consistent breadcrumb navigation across artist pages ([ab6bd81](https://github.com/FlyinPancake/yoink/commit/ab6bd81be0d784996f617a539adbcfa914ff05a6))
* **docker:** add Dockerfile, compose, and CI ([f0b4b28](https://github.com/FlyinPancake/yoink/commit/f0b4b2828d87ac92a39a81988cea3ab1035ee41b))
* Implement download workflow and library fixes ([e2751a6](https://github.com/FlyinPancake/yoink/commit/e2751a650f1c39d3b8cc170bf92a842b69a4ff27))
* Implement import workflows and split shared import module ([148ca9d](https://github.com/FlyinPancake/yoink/commit/148ca9dcd3faf29a7559c8292379fe626b08d0d3))
* implement link artist dialog ([#34](https://github.com/FlyinPancake/yoink/issues/34)) ([e676eab](https://github.com/FlyinPancake/yoink/commit/e676eab5fd7d8a24d42938855b1d89b75809f0b1))
* implement sync_album_tracks and rename sync cascade ([c52c940](https://github.com/FlyinPancake/yoink/commit/c52c9407767162d24f43d385f5c050aba7ef0fd7))
* implemented artist sync ([82aad9a](https://github.com/FlyinPancake/yoink/commit/82aad9aabb6522cb9e46dbe83739e24382ad24bd))
* **library:** ship track-level monitoring and unified navigation ([f106566](https://github.com/FlyinPancake/yoink/commit/f106566b56e27c7a7407316bbad60906cdbb0635))
* Lidarr-style manual import flow with rich candidate matching ([d7a3c87](https://github.com/FlyinPancake/yoink/commit/d7a3c876e54c887daf9e8e3ad5f7edef44e8057c))
* massive ux improvements ([c2bff49](https://github.com/FlyinPancake/yoink/commit/c2bff499fb7af8d6307f35a3a3bf47a6f7908c03))
* **matching:** use MatchStatus enum ([3f2d5b1](https://github.com/FlyinPancake/yoink/commit/3f2d5b16822364146f77325f08655842cbbc3027))
* mobile imrprovements ([0b8809e](https://github.com/FlyinPancake/yoink/commit/0b8809e43955a3e03aaf97047f522dcba71a7718))
* **providers:** add registry, use uuid ids ([fa5dc36](https://github.com/FlyinPancake/yoink/commit/fa5dc36e36c6d90cb052e3d4e063cc00fde93b5d))
* **reconcile:** implement library reconciliation ([da790ed](https://github.com/FlyinPancake/yoink/commit/da790ed3706d4d7cdbf7402eda5535fa129eafe3))
* **server:** backfill album tracks on fetch ([fb6c7f2](https://github.com/FlyinPancake/yoink/commit/fb6c7f2d2044d00f13d2b5d91c031d100909c2d6))
* show track version, explicit badge, and ISRC in tracklist ([f99b672](https://github.com/FlyinPancake/yoink/commit/f99b672be0bea726df0230ef600baa2748726b61))
* stabilize SoulSeek downloads and improve dev workflow ([f1f7b56](https://github.com/FlyinPancake/yoink/commit/f1f7b56acc14e4ae042084c92fe19d7533f2f776))
* **track:** add track quality override handling ([619be66](https://github.com/FlyinPancake/yoink/commit/619be66d4979149625045320cb298c103a2813ab))
* **tracks:** add track_artist and file_path ([516c067](https://github.com/FlyinPancake/yoink/commit/516c067baba8b64b44fc0320d48f4b4a5e67e8d1))
* trigger bio fetch when linking a provider to an artist ([839f667](https://github.com/FlyinPancake/yoink/commit/839f667a88ecc684dbc05205814b7d2ec403b677))
* unmonitor artist when last provider removed ([4976995](https://github.com/FlyinPancake/yoink/commit/4976995d2ec70ff62a19ec662cfac8978c2c2ea8))
* Use migrations instead of schema sync ([e3ad07e](https://github.com/FlyinPancake/yoink/commit/e3ad07ea5d789d16974af5904b9a96e92726cda6))
* Wire track routes and artist image picker ([ca7cbe2](https://github.com/FlyinPancake/yoink/commit/ca7cbe2f2262d2efdb566aa92a25e65b050a9583))


### Bug Fixes

* add ON DELETE CASCADE to all foreign key relations ([9d64203](https://github.com/FlyinPancake/yoink/commit/9d642037273fea7c2b842c65f47ea52beca74fe3))
* **db:** propagate and log db errors ([d883b30](https://github.com/FlyinPancake/yoink/commit/d883b30b95d8a656fe8da5073fb85015ab6ad4e5))
* eliminate UI flashing during SSE updates ([61c6a3c](https://github.com/FlyinPancake/yoink/commit/61c6a3cda7e08e130695a34c3dbc9cdb08bb5dbd))
* error handling and add component tests ([ede993f](https://github.com/FlyinPancake/yoink/commit/ede993f3e2f3ebd4d3fc75145c1dc186334b9af5))
* Fix artist file removal and bio fetching ([1d78ed9](https://github.com/FlyinPancake/yoink/commit/1d78ed95b41e2dadbc50972be50f4865d9dc98e8))
* fixed container image ([6c2373b](https://github.com/FlyinPancake/yoink/commit/6c2373bec56fdf2e19d7fcbab785572ddabba8c2))
* make album sleeve monitor/wanted/acquired state reactive ([31be477](https://github.com/FlyinPancake/yoink/commit/31be477f9fa74b3326be99b05e956a1c7b9d764f))
* properly close EventSource on error before reconnecting ([0cf3b3f](https://github.com/FlyinPancake/yoink/commit/0cf3b3ffed8b0a9869c1a4de86df5a72a2ca717c))
* resolve multiple issues in sync, search, and image handling ([8437b9f](https://github.com/FlyinPancake/yoink/commit/8437b9f8c7a6df88bcee921acbe4cc7c8aea6c2a))
* sanitize path-like artist tags during local library scan ([#23](https://github.com/FlyinPancake/yoink/issues/23)) ([#28](https://github.com/FlyinPancake/yoink/issues/28)) ([a1ead45](https://github.com/FlyinPancake/yoink/commit/a1ead45e1302025660b4d319745d4de9f8e44a5d))
* use extract_year helper for album folder creation ([b7fbf13](https://github.com/FlyinPancake/yoink/commit/b7fbf13a94ca9cc9ffe1ad4f42c5c7258bc8de63))


### Performance Improvements

* **library:** batch album provider link lookups ([18e2b9c](https://github.com/FlyinPancake/yoink/commit/18e2b9c46c4b96e2e37ea636785ec61484e9d67a))


### Code Refactoring

* **api:** inline shared types into server ([8ec4287](https://github.com/FlyinPancake/yoink/commit/8ec42871a74319458383d73ed302250ea37ae871))
* **api:** simplify model conversions ([0f83fdd](https://github.com/FlyinPancake/yoink/commit/0f83fddb62879a8833d9078e193d9433b0092db4))
* **api:** use typed enums for provider/album_type ([25ff536](https://github.com/FlyinPancake/yoink/commit/25ff5365deb60cd88dd5765fa37d2fb8ab398f97))
* break up large modules into focused submodules ([ae25870](https://github.com/FlyinPancake/yoink/commit/ae25870bdd9bc4827c30b9ee50e7623d95868d62))
* centralize download status checks ([bff836d](https://github.com/FlyinPancake/yoink/commit/bff836dbf248e3355d19131c7c0708382e5c5aad))
* **config:** replace envconfig with better-config ([d5a9236](https://github.com/FlyinPancake/yoink/commit/d5a92363790eede6a00072abc476752bb6908497))
* **db:** use sqlx query macros and chrono ([d9021ea](https://github.com/FlyinPancake/yoink/commit/d9021ea6956a63ff7cd98416036360ecca7d40c4))
* decreate log noise ([7c5f282](https://github.com/FlyinPancake/yoink/commit/7c5f282f88635fe7c7b1c7dc6e3a3dbc6a37813d))
* **error:** add AppError and YoinkError ([7df0bfe](https://github.com/FlyinPancake/yoink/commit/7df0bfe12e44d618d0845a3d879959ec152beee6))
* **error:** split error types into modules ([a361b61](https://github.com/FlyinPancake/yoink/commit/a361b615ed42b5615b294e68cfbfa77524e28533))
* **frontend:** inline pending match checks ([3f3ca6e](https://github.com/FlyinPancake/yoink/commit/3f3ca6e6bbbbf19a0762063a30328ecd2b081ef2))
* improve quality select UI ([47d0a1e](https://github.com/FlyinPancake/yoink/commit/47d0a1e0b6ea1821fc0cef040c7bcea952ab6c5f))
* introduce TrackMetadata struct for write_audio_metadata ([7e3a6a4](https://github.com/FlyinPancake/yoink/commit/7e3a6a47a7c86286794e5095be4053e3d8b783c7))
* migrate to proper leptos ([c19c9dc](https://github.com/FlyinPancake/yoink/commit/c19c9dc76a8953b9578f462e8120575d86836c81))
* move more scaffolding components ([c3de8da](https://github.com/FlyinPancake/yoink/commit/c3de8da6f8dc9e9184c5a84fbac522fbbe9031ed))
* Move to React frontend ([#19](https://github.com/FlyinPancake/yoink/issues/19)) ([fe4b3f4](https://github.com/FlyinPancake/yoink/commit/fe4b3f424d133f1457da6b085e91049f8432ac74))
* **providers:** add name_hint to fetch image ([b1af703](https://github.com/FlyinPancake/yoink/commit/b1af7033c89bdac39bfaf227e81e4990d2b1a55f))
* refactor match enums and batch album lookups ([156f11e](https://github.com/FlyinPancake/yoink/commit/156f11e195265c1642bd401f29e2571ad12fba2e))
* remove DbUrl type, use plain strings for URLs ([71c60ee](https://github.com/FlyinPancake/yoink/commit/71c60ee322965b028ff31b37af1c3a80ac40beea))
* replace select dropdown in link dialog with mixed results + filter badges ([57aa273](https://github.com/FlyinPancake/yoink/commit/57aa273c91a9df8c58dce1bcd73ea15a0b856da0))
* **server:** break up actions.rs into smaller parts ([f34c25c](https://github.com/FlyinPancake/yoink/commit/f34c25c3fa5bcaa9a15cfc4660aba52a9b44e01b))
* **server:** refactor actions tests to submodules ([98903f9](https://github.com/FlyinPancake/yoink/commit/98903f933130b2ea32bfda6bb12c03f9b4cf29ad))
* **shared:** improve Quality enum ([c62ca97](https://github.com/FlyinPancake/yoink/commit/c62ca9795a9ccc6bb68c14d055866adfc259aca8))
* simplify remove_artist with cascade deletes ([7b564a3](https://github.com/FlyinPancake/yoink/commit/7b564a35bde4a794ab840687a0ecec42c82f80b0))
* **soulseek:** refactorred soulseek into modules ([777be63](https://github.com/FlyinPancake/yoink/commit/777be6381e4261f34a8d8ff8dd23c2a3e543c0d3))
* **soulseek:** use let-chain and reformat ([8b5628d](https://github.com/FlyinPancake/yoink/commit/8b5628d820916603d89d40845b6c858767a20434))
* split downloads.rs into focused submodules ([e723643](https://github.com/FlyinPancake/yoink/commit/e723643a8927f1196fad7de6ad4ff0607bd5f69b))
* split into multi-crate workspace ([6c96712](https://github.com/FlyinPancake/yoink/commit/6c9671253be0e817ac6e5c547e6263d21a7601bb))
* **test:** extract common test helpers ([2bb63aa](https://github.com/FlyinPancake/yoink/commit/2bb63aad1ae5e70a30331259f7ac0ce788b7203f))
* update AGENTS.md and clean imports ([5501003](https://github.com/FlyinPancake/yoink/commit/5501003daa0a020ff8c7e2c5920ac11bfa5c5303))
* update tracks page ([30be115](https://github.com/FlyinPancake/yoink/commit/30be115983d21476acdfefcc45307d5928fa6ad4))
* use alpine for docker images ([#30](https://github.com/FlyinPancake/yoink/issues/30)) ([013ef69](https://github.com/FlyinPancake/yoink/commit/013ef691c181195e3aaa3081f626dfaae6e720a0))
* use enums for match_kind and providers ([7429318](https://github.com/FlyinPancake/yoink/commit/74293187382730bc623a25292fea1e95573f6a0e))
* use Provider enum for artists ([5e4c3e5](https://github.com/FlyinPancake/yoink/commit/5e4c3e51ace75a7499c38c8fa9e1968712403244))
* use Uuid for ids across codebase ([6191d16](https://github.com/FlyinPancake/yoink/commit/6191d1677749c7ae8210665fb85b74544a65a13a))


### Documentation

* add AI discalimer ([383861f](https://github.com/FlyinPancake/yoink/commit/383861f90b321552a8667fc6e79e0da065abede2))
* Add Contributor Covenant Code of Conduct ([d2f787a](https://github.com/FlyinPancake/yoink/commit/d2f787ae16f20405e04bba4e016291b79f84e7aa))
* add more screenshots ([ffcf752](https://github.com/FlyinPancake/yoink/commit/ffcf752c0ae30ee0e0f8084e6182ed6464d3c2b9))
* add roadmap ([2e20c74](https://github.com/FlyinPancake/yoink/commit/2e20c74a18c1f1c29441a562ee30d024ba2d5afa))
* add ux improvements audit ([a90c15f](https://github.com/FlyinPancake/yoink/commit/a90c15f6f9af0cd9bc6d1eb08401f74fde4d8c64))
* add warning callout about project maturity ([#1](https://github.com/FlyinPancake/yoink/issues/1)) ([944e71a](https://github.com/FlyinPancake/yoink/commit/944e71a9f44e1637241ce9fd7d4025133fae5561))
* improve readme + remove old plan file ([bd3fd9a](https://github.com/FlyinPancake/yoink/commit/bd3fd9adbd9f1d880a0b70c70d58088384891aca))
* mark quality settings as done ([6d657d2](https://github.com/FlyinPancake/yoink/commit/6d657d23110b7c2fe8ff3247c48526061cad9938))
* **tidal:** add docs for tidal provider modules ([db3de29](https://github.com/FlyinPancake/yoink/commit/db3de29655e6c29ef1dff4e845346bc7c95f7150))
* Update CHANGELOG with new feature ([1755275](https://github.com/FlyinPancake/yoink/commit/1755275c79e69507039f30334497dcebc0ac8660))
* update example compose.yaml ([1bd873a](https://github.com/FlyinPancake/yoink/commit/1bd873a9521a63450cebc4e3a41ca01b9316d78f))

## Changelog

This file is managed by release-please.
