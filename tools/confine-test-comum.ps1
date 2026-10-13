# O roteiro da contenção (`confine-test.ps1`) com o token de quem não está
# elevado: o de um administrador com o UAC ligado, sem privilégios e com
# integridade média. É o token que o usuário tem de verdade; o do CI (e o de um
# terminal aberto "como administrador") é o elevado, e o que ele prova não é o
# que o usuário vê.
#
#   keep ia confinar --como-comum -- pwsh -NoProfile -File tools\confine-test-comum.ps1 -Keep C:\caminho\keep.exe
param([string]$Keep = "")

$ErrorActionPreference = "Continue"
Write-Host "o token deste roteiro:"
whoami /groups
whoami /priv
& (Join-Path $PSScriptRoot "confine-test.ps1") -Keep $Keep
exit $LASTEXITCODE
