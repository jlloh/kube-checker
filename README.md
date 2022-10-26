## kube-checker
### Introduction
* A utility script to capture the "state" of your cluster
* It starts from listing all the pods in your cluster, then checking what owns the pod (e.g. a replicaset)
* Afterwards, it aggregates the pods by `object`, `containers`
* There are two primary modes of operation, `filter` and `disable-filter`

### Filter Enabled
* Simple script that checks for "best practices" within a cluster (currently EKS)
  * All Statefulsets and Deployments have node selectors (node_selector_check)
  * All Statefulsets and Deployments have resource requests defined (qos_check)
  * All Statefulsets and Deployments are using ECR images (image_check)
* Run the script with:
```bash
kube-checker --print-table
```
* This will print a table similar to the one below (albeit with many more fields). To be fixed later.
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

### Filter Disabled
* This mode can be used to capture a snapshot of your cluster, to tell you which are the biggest "objects", accounting for all sidecars (including ones that were injected with webhooks)
* Run the command:
```bash
kube-checker --disable-filter --generate-csv
```
* This will generate two files 
  * `result_by_container_name.csv` - aggregated by container name
  * `result_by_object.csv` - aggregated by object (e.g. replicaset)

##  Usage
```
kube-checker/target/debug/kube-checker -h
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

## TODO
* Simplify output of print-table to reduce verbosity so it can fit on screen
* Recurse and aggregate by top level object? E.g. pod -> replicaset -> deployment -> CRD. Would be ideal if we could get the CRD name