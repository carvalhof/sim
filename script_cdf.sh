#!/bin/bash

FILENAME=$1

cat $FILENAME | sort -n > .sorted

NMAX=`wc -l $FILENAME | cut -d' ' -f1`
MAX=$(( NMAX - 1 ))
cat .sorted | uniq --count | awk -v MAX="$MAX" 'BEGIN{sum=0}{print $2,$1,(sum/MAX); sum=sum+$1}' > $FILENAME.cdf
#cat .sorted | uniq --count | awk -v MAX="$MAX" 'BEGIN{sum=0}{print $2,$1,sum; sum=sum+$1}' > $FILENAME.cdf
rm .sorted
