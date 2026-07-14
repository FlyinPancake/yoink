# Changelog

## [0.1.7](https://github.com/FlyinPancake/yoink/compare/v0.1.6...v0.1.7) (2026-07-13)


### Bug Fixes

* **tidal:** add lossless manifest support ([#78](https://github.com/FlyinPancake/yoink/issues/78)) ([37d82ab](https://github.com/FlyinPancake/yoink/commit/37d82abd211796a30b60df2ba67b83aa6beb44d4))


### Code Refactoring

* generic jobs support ([#64](https://github.com/FlyinPancake/yoink/issues/64)) ([bdbd4fb](https://github.com/FlyinPancake/yoink/commit/bdbd4fb690146c87ddd62a97710e24d1c7558051))
* **providers:** migrate errors to snafu ([#75](https://github.com/FlyinPancake/yoink/issues/75)) ([e5693e7](https://github.com/FlyinPancake/yoink/commit/e5693e7f7fb1822044d81f3ef223331868957d5f))

## [0.1.6](https://github.com/FlyinPancake/yoink/compare/v0.1.5...v0.1.6) (2026-04-24)


### Bug Fixes

* update instances list provider ([eacee90](https://github.com/FlyinPancake/yoink/commit/eacee90ec9f35419fc4d5d615cc7676f832435ad))

## [0.1.5](https://github.com/FlyinPancake/yoink/compare/v0.1.4...v0.1.5) (2026-04-20)


### Features

* graceful shutdown ([#39](https://github.com/FlyinPancake/yoink/issues/39)) ([10ec2f6](https://github.com/FlyinPancake/yoink/commit/10ec2f6081acd880dd64dad84d2ccb0613053203))
* graceful shutdown first implementation ([10ec2f6](https://github.com/FlyinPancake/yoink/commit/10ec2f6081acd880dd64dad84d2ccb0613053203))


### Bug Fixes

* **docs:** add favicon to docs page ([ac1f328](https://github.com/FlyinPancake/yoink/commit/ac1f3284a7a3a919d2a1efcc11d53f3900b12516))
* **docs:** fix docs site build ([6b2aaaa](https://github.com/FlyinPancake/yoink/commit/6b2aaaa8add74dd535a23baa82158d13341ddba2))
* **docs:** fix images in vercel ([77ff4ba](https://github.com/FlyinPancake/yoink/commit/77ff4ba10a8071fc89c61f889360ccd0ada9ef16))
* re-add dotenv support ([#40](https://github.com/FlyinPancake/yoink/issues/40)) ([eeedfaf](https://github.com/FlyinPancake/yoink/commit/eeedfaf8365ffc0f86854be1818328377f068292))
* show intial admin password on launch ([#44](https://github.com/FlyinPancake/yoink/issues/44)) ([9ef06ea](https://github.com/FlyinPancake/yoink/commit/9ef06ea7f6ad7d4150687a458b38511453c4e756))
* use sort_by_key ([38e1a88](https://github.com/FlyinPancake/yoink/commit/38e1a886b13e1e77a149c022d560af6bb8aa1e99))


### Performance Improvements

* **ui:** use react query for auth status ([#41](https://github.com/FlyinPancake/yoink/issues/41)) ([c5aadbf](https://github.com/FlyinPancake/yoink/commit/c5aadbfab209fdf1125861473dd627bacf7f3e82))


### Code Refactoring

* use sort_by_key where applicable ([#45](https://github.com/FlyinPancake/yoink/issues/45)) ([38e1a88](https://github.com/FlyinPancake/yoink/commit/38e1a886b13e1e77a149c022d560af6bb8aa1e99))


### Documentation

* add docs site ([#37](https://github.com/FlyinPancake/yoink/issues/37)) ([e0de935](https://github.com/FlyinPancake/yoink/commit/e0de935fb5dcd303f346704f0f1bc049426f2eb3))
* document postgres usage ([#43](https://github.com/FlyinPancake/yoink/issues/43)) ([cf7e78d](https://github.com/FlyinPancake/yoink/commit/cf7e78d7bae20cbe9493864f8ea4522ac71a784e))
* point to docs site in readme.md ([920b690](https://github.com/FlyinPancake/yoink/commit/920b690d16e6d04282ea92ee3cbfaafcce6aebee))

## [0.1.4](https://github.com/FlyinPancake/yoink/compare/v0.1.3...v0.1.4) (2026-04-11)


### Features

* implement link artist dialog ([#34](https://github.com/FlyinPancake/yoink/issues/34)) ([e676eab](https://github.com/FlyinPancake/yoink/commit/e676eab5fd7d8a24d42938855b1d89b75809f0b1))
* Use migrations instead of schema sync ([e3ad07e](https://github.com/FlyinPancake/yoink/commit/e3ad07ea5d789d16974af5904b9a96e92726cda6))


### Bug Fixes

* sanitize path-like artist tags during local library scan ([#23](https://github.com/FlyinPancake/yoink/issues/23)) ([#28](https://github.com/FlyinPancake/yoink/issues/28)) ([a1ead45](https://github.com/FlyinPancake/yoink/commit/a1ead45e1302025660b4d319745d4de9f8e44a5d))
* use extract_year helper for album folder creation ([b7fbf13](https://github.com/FlyinPancake/yoink/commit/b7fbf13a94ca9cc9ffe1ad4f42c5c7258bc8de63))


### Code Refactoring

* use alpine for docker images ([#30](https://github.com/FlyinPancake/yoink/issues/30)) ([013ef69](https://github.com/FlyinPancake/yoink/commit/013ef691c181195e3aaa3081f626dfaae6e720a0))

## Changelog

This file is managed by release-please.
