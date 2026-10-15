# Self-signed packaging. Private material stays out of vpk's command line/logs.
param(
    [Parameter(Mandatory)][string]$CertificatePath,
    [Parameter(Mandatory)][string]$ArgumentsPath,
    [Parameter(Mandatory)][string]$PublicCertificatePath
)
$ErrorActionPreference = 'Stop'
$storeName = "TantrixSigning-$([guid]::NewGuid().ToString('N'))"
$store = [System.Security.Cryptography.X509Certificates.X509Store]::new($storeName, 'CurrentUser')
$store.Open('ReadWrite')
$rootAdded = $false
$certificate = $null
$verifyDir = Join-Path (Split-Path $ArgumentsPath) 'verify-portable'
try {
    Write-Output 'Importing the signing certificate'
    $password = ConvertTo-SecureString $env:WINDOWS_CERTIFICATE_PASSWORD -AsPlainText -Force
    $certificate = Import-PfxCertificate -FilePath $CertificatePath -Password $password `
        -CertStoreLocation "Cert:\CurrentUser\$storeName"
    if (@($certificate).Count -ne 1 -or -not $certificate.HasPrivateKey) {
        throw 'The PFX must contain exactly one signing certificate with its private key'
    }
    $thumbprint = $certificate.Thumbprint
    if ($certificate.Subject -ne $certificate.Issuer) { throw 'Expected a self-signed certificate' }
    if ($certificate.NotAfter -le (Get-Date) -or $certificate.NotBefore -gt (Get-Date)) {
        throw 'The signing certificate is expired or not yet valid'
    }
    $basicConstraints = $certificate.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.19' }
    if (-not $basicConstraints) {
        throw 'Signing certificate must explicitly contain end-entity Basic Constraints; replace the PFX'
    }
    $constraints = [System.Security.Cryptography.X509Certificates.X509BasicConstraintsExtension]::new(
        $basicConstraints, $basicConstraints.Critical)
    if ($constraints.CertificateAuthority -or $constraints.HasPathLengthConstraint) {
        throw 'Signing certificate must be an end entity (CA=FALSE) without a path-length constraint; replace the PFX'
    }
    Write-Output 'Signing certificate Basic Constraints: end entity, no path-length constraint'
    Export-Certificate -Cert $certificate -FilePath $PublicCertificatePath | Out-Null
    Write-Output 'Signing certificate imported; preparing temporary trust'
    # Machine trust avoids the user-store confirmation dialog in headless CI.
    # GitHub-hosted Windows runners are elevated; remove this trust afterward.
    if (-not (Test-Path "Cert:\LocalMachine\Root\$thumbprint")) {
        Import-Certificate -FilePath $PublicCertificatePath -CertStoreLocation Cert:\LocalMachine\Root | Out-Null
        $rootAdded = $true
    }
    $packArguments = @(Get-Content -Raw $ArgumentsPath | ConvertFrom-Json)
    $signParameters = "/s $storeName /sha1 $thumbprint /fd SHA256 /tr http://timestamp.digicert.com /td SHA256"
    Write-Output 'Packaging and signing with Velopack'
    & vpk @packArguments --signParams $signParameters
    if ($LASTEXITCODE -ne 0) { throw 'Velopack packaging/signing failed' }

    $outputIndex = [array]::IndexOf($packArguments, '--outputDir')
    if ($outputIndex -lt 0) { throw 'Missing packaging output directory' }
    $output = $packArguments[$outputIndex + 1]
    Expand-Archive -Path (Get-Item "$output/*-Portable.zip").FullName -DestinationPath $verifyDir
    $installers = @(Get-ChildItem "$output/*-Setup.exe")
    $executables = @(Get-ChildItem $verifyDir -Recurse -Filter *.exe)
    if ($installers.Count -ne 1 -or $executables.Count -eq 0) { throw 'Missing signed outputs' }
    foreach ($file in ($installers + $executables)) {
        $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
        Write-Output ("Signature: {0}; Status={1}; Reason={2}; Signer={3}; ExpectedSigner={4}" -f
            $file.Name, $signature.Status, $signature.StatusMessage,
            $signature.SignerCertificate.Thumbprint, $thumbprint)
        if ($signature.Status -ne 'Valid') {
            # Run before removing temporary trust; diagnostics must not mask the failure.
            try {
                $signTool = Get-Command signtool.exe -ErrorAction SilentlyContinue
                $signToolPath = if ($signTool) { $signTool.Source } else {
                    $sdkBin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
                    Get-ChildItem "$sdkBin/*/x64/signtool.exe" |
                        Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
                }
                if ($signToolPath) {
                    & $signToolPath verify /pa /all /v $file.FullName
                    Write-Output "SignTool verification exit code: $LASTEXITCODE"
                } else {
                    Write-Warning 'SignTool was not found for additional signature diagnostics'
                }
            } catch {
                Write-Warning "Could not run SignTool diagnostics: $_"
            }
            throw ("Signature verification failed: {0}; Status={1}; Reason={2}; Signer={3}" -f
                $file.Name, $signature.Status, $signature.StatusMessage,
                $signature.SignerCertificate.Thumbprint)
        }
        if (($file.Name -eq 'tantrix.exe' -or $file.Name -like '*-Setup.exe') -and
            $signature.SignerCertificate.Thumbprint -ne $thumbprint) {
            throw "Unexpected signing identity: $($file.Name)"
        }
    }
} catch {
    # Report the packaging failure before cleanup, so cleanup cannot hide it.
    Write-Error -ErrorRecord $_ -ErrorAction Continue
    throw
} finally {
    try {
        if ($rootAdded) { Remove-Item -Path "Cert:\LocalMachine\Root\$thumbprint" -Force -Confirm:$false }
        foreach ($item in (Get-ChildItem "Cert:\CurrentUser\$storeName")) {
            # DeleteKey is a dynamic parameter: bind the Cert: path explicitly.
            Remove-Item -Path "Cert:\CurrentUser\$storeName\$($item.Thumbprint)" -DeleteKey -Force -Confirm:$false
        }
    } finally {
        $store.Close()
        Remove-Item "HKCU:\Software\Microsoft\SystemCertificates\$storeName" -Recurse -Force -ErrorAction SilentlyContinue
        Remove-Item $verifyDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
