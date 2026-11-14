{{- define "mb.labels" -}}
app.kubernetes.io/part-of: music-browser
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version }}
{{- end }}

{{- define "mb.tag" -}}
{{ .Values.image.tag | default .Chart.AppVersion }}
{{- end }}

{{- define "mb.podCommon" -}}
{{- with .Values.arch }}
nodeSelector:
  kubernetes.io/arch: {{ . }}
{{- end }}
securityContext:
  {{- toYaml .Values.podSecurityContext | nindent 2 }}
{{- end }}

{{- define "mb.podAnnotations" -}}
sidecar.istio.io/inject: {{ .Values.mesh.sidecars | quote }}
{{- if .Values.mesh.sidecars }}
sidecar.istio.io/proxyCPU: {{ .Values.mesh.proxyResources.requests.cpu | quote }}
sidecar.istio.io/proxyMemory: {{ .Values.mesh.proxyResources.requests.memory | quote }}
sidecar.istio.io/proxyMemoryLimit: {{ .Values.mesh.proxyResources.limits.memory | quote }}
{{- end }}
{{- end }}

{{- define "mb.dataVolume" -}}
- name: data
  {{- if .p.hostPath }}
  hostPath:
    path: {{ .p.hostPath }}
    type: DirectoryOrCreate
  {{- else }}
  persistentVolumeClaim:
    claimName: {{ .name }}-data
  {{- end }}
{{- end }}

{{- define "mb.pvc" -}}
{{- if not .p.hostPath }}
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: {{ .name }}-data
  annotations:
    helm.sh/resource-policy: keep
spec:
  accessModes: [ReadWriteOnce]
  {{- with .p.storageClass }}
  storageClassName: {{ . }}
  {{- end }}
  resources:
    requests:
      storage: {{ .p.size }}
{{- end }}
{{- end }}
