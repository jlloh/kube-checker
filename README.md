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

### TODO
* check daemonsets as well for 2nd and 3rd criteria above
* image_check can be improved to include all non-dockerhub sources, e.g. GCR