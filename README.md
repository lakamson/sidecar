# sidecar

## Why sidecar?

We need an additional binary which we can offload the heavy lifting to and use that instead of just using the normal binary as its slow as shit


## onnx runtime and wtf
- We need a onnx runtime which we are checking into the repository to make sure that we can enable ort
 to work as we want it to
- we could check in this binary into the repo and have it built along with the editor to keep both of them intact and make sure that we are not regressing anywhere
- So for the binary to work we need the following things to be present at the
 right location: models folder and also the libonnxruntime file depending on the platform
- once we can get these things sorted we are in a good position to run and package
the binary 

## We are not going to parallelize anything, we are proud of being lazy
- fix the speed etc when we hit issues with it

## How to install sqlx and migrations
- for sqlx install it using `cargo install sqlx`
- and then for the migrations which are present in the ./migrations folder where we have Cargo.toml, we need to add migrations using `sqlx migrate add {blah}`
- you can use the following command to do the migrations etc:
- cargo sqlx prepare --database-url=sqlite://codestory.db

## Qdrant binary and where to download
- To download the binaries, you can visit this: https://github.com/qdrant/qdrant/releases/tag/v1.2.0
- download from here and update the binary where required


## What keys are important here?
- We need to have a single key which can map back to the semantic algorithm we are using, cause tantivy is sensitive to changes
 in the keys
- Then we need a key to identify the file using the file path (we can use that to lookup everything about a file and update things)
- Lastly we also need a key which can be used to track the commit hash associated with the repo when we are indexing
- And another key which is the hash of the file content in the file, this will be useful to make sure that we can see if things have changed or not and decide accordingly

Database structure:
file_cache: file_path, repo_ref, tantivy_cache_key, file_hash
chunk_cache: file_path, repo_ref, chunk_hash, line_start, line_end, tantivy_cache_key


## What are the important files where we need to rebuild the database again?
- semantic_search/schema.rs is one of them