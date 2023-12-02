cargo build --bin webserver --release

pathsToZip="sidecar/onnxruntime/ sidecar/qdrant/ target/release/webserver.exe sidecar/models/"

# Destination of the zip file
zipFileDestination="sidecar.7z"

# Use 7z command to create the archive
7z a -tzip $zipFileDestination $pathsToZip