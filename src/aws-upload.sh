#!/usr/bin/env bash

write_to_db() {
   echo writing $file to db 
}

cleanup() {
    echo cleaning up $file and $file.md5
    rm $1 $2
}

file=$1

md5sum $file > $file.md5

#(tar -cvz -f - $file $file.md5 | ./encrypt.sh | ./upload.sh) && write_to_db && cleanup $file $file.md5

#tar -cvz -f - $file $file.md5

# gpg:
# echo 'plaintext' | gpg -c -o -
# /usr/bin/gpg --batch --no-tty --encrypt --cipher-algo AES256 --compress-algo none -r something

# aws:
# echo "hello world" | aws s3 cp - s3://some-bucket/hello.txt
# ^^^^
#if DEEPARCHIVE:
#aws_string = '/usr/local/bin/aws s3 cp --profile deeparchive --storage-class DEEP_ARCHIVE '+encrypted_file_path+' s3://some-bucket/'+file_year+'/'+file_month+'/'
#else:
#aws_string = '/usr/bin/mtglacier --config /root/.aws/glacier.cfg --journal "/var/log/glacier/'+ymd+'.log" --vault "' \
#+VAULT+'" --dir '+LOCAL_DIR+' --filename ' + encrypted_file_path + ' --concurrency 30 upload-file'
