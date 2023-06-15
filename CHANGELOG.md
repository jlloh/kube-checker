# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.1] - 2022-06-15
### Changed
* Changed to rusttls

## [0.3.0] - 2021-10-26
### Added
* Major refactor to start from pod_level objects and work backwards, to capture containers injected from webhooks like consul/envoy sidecars

## [0.2.0] - 2021-04-06
### Added
* Added clap to accept command line arguments to control behaviour
  * --disable-filter
  * --generate-csv
  * --print-table
* Added option to disable filtering to print out all pod information
* Added some metrics for CPU request, replicas, etc.