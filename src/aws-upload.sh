#!/usr/bin/env bash

S3BUCKET="s3doj"
S3FOLDER="upload-test"
DBTABLE="files"

write_to_db() {
    fullpath=$1
    filename=$(basename $fullpath)
    directory=$(dirname $fullpath)

    dbinsert="INSERT INTO $DBTABLE (host, filename, fullpath, directory, upload_finished) VALUES\
             (\"$(hostname -f)\", \"$filename\", \"$fullpath\", \"$directory\", NOW())"

    echo writing info for $f to the database
    # dbhost, database, user, pass in ~/.my.cnf under [client_test]
    mysql --defaults-group-suffix=_test -e "$dbinsert" || (echo failed to write to database && return 1)
}

cleanup() {
    f=$1
    echo cleaningup lockfile for $f
    rm $f.lock
    echo would remove $f
    # Don't actually remove it until we're ready for prime time
    #rm $1
}

path=$1
file=$(basename $path)
directory=$(dirname $path)

# WARNING: Big assumption here! We are passed a path that looks like /data/aws/YYYY-mm-dd/filename
prefix="/data/aws/"
#if [[ $path != $prefix* ]]; then
#    echo "$path must start with $prefix"
#    exit 1
#fi
datedir=${directory#$prefix} # get rid of prefix, leaving only the date portion
year=${datedir%-*-*}
month=$(echo $datedir | sed -r 's/[0-9]{4}-([0-9]{2})-[0-9]{2}/\1/')

#echo path $1
#echo file $file
#echo directory $directory
#echo datedir $datedir
#echo year $year
#echo month $month
#exit

# work out of the same directory as the file
cd $directory

# Create a lockfile before starting the upload pipeline.
# If the lockfile for this file already exists then we bail.
# The lockfile is only cleaned up after a successful upload and db insert.
# This keeps us from retrying any failed uploads
test -f $file.lock && echo lockfile for $file already exists && exit 1
touch $file.lock

# Log start
starttime="$(date "+%Y-%m-%d %T")"
echo "$starttime: START FILE: $file"

# create a file with our md5 checksum
md5sum $file > $file.md5

# tarball-encrypt-upload pipeline
# if we successfully upload a file then we write a record to the database
# and then remove the original file
(tar -cvz -f - $file.md5 $file |\
gpg --batch --no-tty --encrypt --cipher-algo AES256 --compress-algo none -r 1539150C -o - |\
#aws s3 cp --profile deeparchive --storage-class DEEP_ARCHIVE - s3://$S3BUCKET/$S3FOLDER/$file.tgz.crypt)\
aws s3 cp --profile deeparchive - s3://$S3BUCKET/$S3FOLDER/$file.tgz.crypt)\
&& write_to_db $path\
&& cleanup $file

# cleanup the md5 regardless
test -f $file.md5 && (echo cleaning up $file.md5; rm $file.md5)

# Log end
endtime="$(date "+%Y-%m-%d %T")"
echo "$endtime: END FILE: $file"

#------------------------------------
# Some Notes
#------------------------------------

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

#CREATE TABLE `files` (
#  `id` int(11) NOT NULL AUTO_INCREMENT,
#  `host` varchar(50) DEFAULT NULL,
#  `filename` varchar(100) NOT NULL,
#  `fullpath` varchar(250) NOT NULL,
#  `directory` varchar(250) NOT NULL,
#  `upload_finished` datetime DEFAULT NULL,
#  PRIMARY KEY (`id`),
#  UNIQUE KEY `filename` (`filename`),
#  KEY `host` (`host`)
#) ENGINE=InnoDB AUTO_INCREMENT=6 DEFAULT CHARSET=utf8mb4
