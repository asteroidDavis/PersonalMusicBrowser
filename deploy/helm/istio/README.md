# Istio for the music-browser chart

Pinned official charts; upstream defaults except on small hosts:

```sh
ISTIO_VERSION=1.30.8
helm repo add istio https://istio-release.storage.googleapis.com/charts
helm install istio-base istio/base -n istio-system --create-namespace --version $ISTIO_VERSION
helm install istiod istio/istiod -n istio-system --version $ISTIO_VERSION \
  -f deploy/helm/istio/values-istiod-mini.yaml          # mini only
helm install istio-ingress istio/gateway -n istio-ingress --create-namespace --version $ISTIO_VERSION \
  -f deploy/helm/istio/values-gateway-mini.yaml         # mini only
```

The `istio/gateway` release named `istio-ingress` gets the label
`istio: ingress`, which the chart's `gateway.selector` matches.
