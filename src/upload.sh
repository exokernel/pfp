#!/usr/bin/env bash

#echo uploading to aws

while read line
do
  echo uploading "$line"
done #< "/dev/stdin"