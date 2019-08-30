#!/usr/bin/env bash

# packages to check from script argument (space separated)
readarray -t PACKAGES <<< "$(printf '%s\n' $1 | LC_COLLATE=C sort -u)"

# installed packages
readarray -t INSTALLED <<< "$(yum -C list installed | grep --color=never -oE "^[_[:alnum:]\-]+")"
unset 'INSTALLED[0]'

# missing packages (not installed)
readarray -t MISSING <<< "$(LC_COLLATE=C comm -13 <(printf '%s\n' "${INSTALLED[@]}") <(printf '%s\n' "${PACKAGES[@]}"))"

# formatting JSON output
printf "["
idx=1
len=${#MISSING[@]}
for p in ${MISSING[@]}
do
  if [ ${idx} -eq ${len} ]
  then
    printf "\"%s\"" ${p}
  else
    printf "\"%s\"," ${p}
  fi
  ((idx++))
done
printf "]\n"







