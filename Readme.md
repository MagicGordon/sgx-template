# A SGX Sample 
 Refer to projects https://github.com/apache/incubator-teaclave-sgx-sdk/tree/master/samplecode/mutual-ra and https://github.com/Phala-Network/phala-pruntime

## Requirements

To use this code sample, one needs to register at [Intel website](https://api.portal.trustedservices.intel.com/EPID-attestation) for dev IAS service access(must unlinkable). Once the registration is finished, the following stuff should be ready:

1. An SPID assigned by Intel
2. IAS API Key assigned by Intel

Both of these information could be found in the new [Intel Trusted Services API Management Portal](https://api.portal.trustedservices.intel.com/developer). Please log into this portal and switch to "Manage subscriptions" page on the top right corner to see your SPID and API keys. Either primary key or secondary key works.

Save them to `bin/spid.txt` and `bin/key.txt` respectively. Size of these two files should be 32 or 33.

## Run

```
make
cd bin
./app 
```

open other terminal, run: 
```
curl -XPOST 127.0.0.1:8000/ -H 'Content-Type: application/json' --data '{"operation":2}'
```

You should see a response:

```
{"result":2}
```
