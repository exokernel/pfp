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
    filename=$(basename "$fullpath")
    directory=$(dirname "$fullpath")

    dbinsert="INSERT INTO $DBTABLE (host, filename, fullpath, directory, upload_finished) VALUES\
             (\"$(hostname -f)\", \"$filename\", \"$fullpath\", \"$directory\", NOW())"

    tsecho "writing info for $filename to the database"
    # dbhost, database, user, pass in ~/.my.cnf under [client_test]
    mysql --defaults-group-suffix=_test -e "$dbinsert" || (>&2 tsecho "failed to write to database" && return 1)
}

cleanup() {
    f=$1
    tsecho "cleaning up lockfile for $f"
    rm "$f".lock
    # Don't actually remove it until we're ready for prime time
    #tsecho "would remove $f"
    tsecho "removing $f"
    rm "$f"
}

path=$1
test -f "$path" || bail "no such file: $path"

file=$(basename "$path")
fn_noext=${file%.*}
fn_noext=${fn_noext#show_}
directory=$(dirname "$path")

# WARNING: Big assumption here! We are passed a path that looks like /data/aws/YYYY-mm-dd/filename
prefix="/data/aws/"
if [[ $path != $prefix* ]]; then
    echo "$path must start with $prefix"
    exit 1
fi
datedir=${directory#$prefix} # get rid of prefix, leaving only the date portion
year=${datedir%-*-*}
month=$(echo "$datedir" | sed -r 's/[0-9]{4}-([0-9]{2})-[0-9]{2}/\1/')
S3FOLDER="$year/$month"

#echo path $1
#echo file $file
#echo fn $fn_noext
#echo directory $directory
#echo datedir $datedir
#echo year $year
#echo month $month
#echo s3folder $S3FOLDER
#exit

# work out of the same directory as the file
cd "$directory" || bail "failed to chdir to $directory"

aws s3 ls --profile deeparchive s3://"$S3BUCKET"/"$S3FOLDER"/"$fn_noext" >/dev/null && bail "$file already exists in s3 bucket"

# Create a lockfile before starting the upload pipeline.
# If the lockfile for this file already exists then we bail.
# The lockfile is only cleaned up after a successful upload and db insert.
# This keeps us from retrying any failed uploads
test -f "$file".lock && bail "lockfile for $file already exists"
touch "$file".lock

# Log start
tsecho "START FILE: $file"

# create a file with our md5 checksum
md5sum "$file" > "$file".md5 || bail "failed to create md5 file for $file"

# tarball-encrypt-upload pipeline
# if we successfully upload a file then we write a record to the database
# and then remove the original file
#aws s3 cp --profile deeparchive - s3://$S3BUCKET/$S3FOLDER/"$fn_noext".test)\
starttime_ms="$(date "+%s%3N")"
endtime_ms=$starttime_ms
tar -cz -f - "$file".md5 "$file" |\
gpg --batch --no-tty --encrypt --cipher-algo AES256 --compress-algo none -r 1539150C -o - |\
aws s3 cp --profile deeparchive --storage-class DEEP_ARCHIVE - s3://$S3BUCKET/"$S3FOLDER"/"$fn_noext"\
&& endtime_ms="$(date "+%s%3N")" && write_to_db "$path"\
&& cleanup "$file"

# cleanup the md5 regardless
test -f "$file".md5 && (tsecho "cleaning up $file.md5"; rm "$file".md5)

# Log end
duration_ms=$((endtime_ms - starttime_ms))
tsecho "END FILE: $file DURATION: ${duration_ms}ms"

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
