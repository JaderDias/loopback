# loopback

Create a rust project with all the code in as many separate files as possible, that uses dotenvy to load its config 

PUBLIC_IP_ADDRESS
ALTERNATIVE_INTERFACE
MAX_PAYLOAD_SIZE
MIN_PAYLOAD_SIZE
INTERVAL_MILLIS
LISTEN_PORT

The program will listen on the specified port for its own periodic requests with the do not fragment flag set that go through the internet using the alternative network interface specified and come back through the default one

The program should also listen for when ICMP responds that the packet was rejected because it was too big to be sent without fragmentation
