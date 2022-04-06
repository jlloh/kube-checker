## kube-checker
* Simple script that checks for "best practices" within a cluster (currently EKS)
  * All Statefulsets and Deployments have node selectors (node_selector_check)
  * All Statefulsets and Deployments have resource requests defined (qos_check)
  * All Statefulsets and Deployments are using ECR images (image_check)

```
+-----------------------+------------+--------------+-------------------+----------+-----------+
|          name         |  type_of   |container_name|node_selector_check| qos_check|image_check|
+-----------------------+------------+--------------+-------------------+----------+-----------+
|      cert-manager     | deployment | cert-manager |       false       |   false  |   false   |
+-----------------------+------------+--------------+-------------------+----------+-----------+
|cert-manager-cainjector| deployment | cert-manager |       false       |   false  |   false   |
+-----------------------+------------+--------------+-------------------+----------+-----------+
|  cert-manager-webhook | deployment | cert-manager |       false       |   false  |   false   |
+-----------------------+------------+--------------+-------------------+----------+-----------+
```

Written in Rust.

## Usage
```
kube-checker/target/debug/kube-checker -h
kube-checker 0.2.0
Clap command line arguments

USAGE:
    kube-checker [OPTIONS]

OPTIONS:
        --disable-filter    Disable filters so that all pods are displayed
        --generate-csv      Generate a csv file in the directory, foo.csv
    -h, --help              Print help information
        --print-table       Print out the table to stdout
    -V, --version           Print version information
```

### TODO
* Filters for specific checks? And also there are too many columsn now 