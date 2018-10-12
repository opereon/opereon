#!/usr/bin/env bash

export ENV_VAR1=var1
export ENV_VAR2=var2

cat > /dev/shm/op_script <<-'%%EOF%%'
#!/usr/bin/python
import sys;
print sys.argv
%%EOF%%
chmod +x /dev/shm/op_script

(/dev/shm/op_script "$@")

STATUS=$?
rm /dev/shm/op_script
exit $STATUS
