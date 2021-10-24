#!/usr/bin/env bash

S3BUCKET="s3doj"
S3FOLDER="upload-test"
DBTABLE="files"

bail() {
    msg=$1
    >&2 tsecho "$msg"
    exit 1
}

tsecho() {
    message=$1
    echo "$(date "+%Y-%m-%d %T"): $message"
}

write_to_db() {
    fullpath=$1
    filename=$(basename $fullpath)
    directory=$(dirname $fullpath)

    dbinsert="INSERT INTO $DBTABLE (host, filename, fullpath, directory, upload_finished) VALUES\
             (\"$(hostname -f)\", \"$filename\", \"$fullpath\", \"$directory\", NOW())"

    echo writing info for $filename to the database
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
test -f $file.lock && bail "lockfile for $file already exists"
touch $file.lock

# Log start
#starttime="$(date "+%Y-%m-%d %T")"
tsecho "START FILE: $file"

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
#endtime="$(date "+%Y-%m-%d %T")"
tsecho "END FILE: $file"

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
#) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4
