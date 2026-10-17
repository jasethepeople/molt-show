# MOLT Show Kubernetes Deployment Bundle

This directory drafts a Kubernetes-first deployment for the capacity-aware admission-control layer.

## Included

- `redis-statefulset.yaml`: durable Redis instance with AOF persistence for episode queue streams, reservations, rollout state, and audit metadata.
- `admission-controller.yaml`: admission controller Deployment, Service, PodDisruptionBudget, and health endpoints.
- `monitoring.yaml`: PrometheusRule and ServiceMonitor resources for clusters using Prometheus Operator.
- `grafana-dashboard.yaml`: dashboard JSON packaged as a Grafana sidecar-discoverable ConfigMap.
- `admission-config.yaml`: queue, slot, retry, rollout, and integrity policy.
- `admission-secret-template.yaml`: placeholder Secret only; do not use live credentials from source control.

## Prerequisites

1. Kubernetes 1.27+ with a dynamic StorageClass.
2. Kustomize or `kubectl apply -k`.
3. Prometheus Operator for `ServiceMonitor` and `PrometheusRule`.
4. Grafana with dashboard sidecar discovery enabled for the label `grafana_dashboard: "1"`.
5. Published admission-controller image and Ollama worker image/service. The image names in the manifests are intentionally placeholders.
6. Secret management through Sealed Secrets, External Secrets, or a deployment-time Secret injection mechanism.

## Configure before applying

Replace these placeholders:

- `ghcr.io/jasethepeople/molt-admission-controller:REPLACE_WITH_TAG`
- `ghcr.io/jasethepeople/molt-ollama-worker:REPLACE_WITH_TAG`
- `storageClassName: REPLACE_WITH_STORAGE_CLASS`
- Secret values in `admission-secret-template.yaml`
- Redis and worker endpoints if they are managed outside this namespace

The current manifests provision the durable queue and monitoring components. They do not create a public ingress, TLS certificate, GPU device plugin, or external object store.

## Apply

```bash
kubectl apply -k deploy/k8s
kubectl -n molt-show rollout status statefulset/molt-queue
kubectl -n molt-show rollout status deployment/molt-admission-controller
kubectl -n molt-show get pods,svc,pvc
```

## Remove

```bash
kubectl delete -k deploy/k8s
```

Deleting the StatefulSet does not automatically delete the PVC. Review and delete the PVC only after the queue and audit data have been backed up.

## Production notes

- Use a managed Redis-compatible service or a Redis Sentinel/Cluster topology for production HA.
- Keep the admission controller's `global_in_flight` lower than the sum of worker slots until a capacity benchmark promotes a higher value.
- Do not expose Redis outside the cluster.
- Run a synthetic smoke episode and gate regression fixture before promoting a new worker configuration.
- The admission controller must be the only production path allowed to launch episodes.
