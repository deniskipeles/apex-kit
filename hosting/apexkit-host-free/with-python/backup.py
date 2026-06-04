import os
import sys
import subprocess
import boto3
from botocore.client import Config
from flask import Flask, jsonify
from datetime import datetime

S3_ENDPOINT_URL = os.getenv('S3_ENDPOINT_URL')
S3_ACCESS_KEY = os.getenv('S3_ACCESS_KEY')
S3_SECRET_KEY = os.getenv('S3_SECRET_KEY')
S3_BUCKET_NAME = os.getenv('S3_BUCKET_NAME')
S3_REGION = os.getenv('S3_REGION', 'auto')

PREFIX = 'apexkit_backup_'
MAX_BACKUPS_TO_KEEP = int(os.getenv('MAX_BACKUPS', '5'))

def get_s3_client():
    if not all([S3_ENDPOINT_URL, S3_ACCESS_KEY, S3_SECRET_KEY, S3_BUCKET_NAME]):
        print("⚠️  S3 credentials missing. Skipping cloud storage operations.")
        return None
    
    return boto3.client('s3',
        endpoint_url=S3_ENDPOINT_URL,
        aws_access_key_id=S3_ACCESS_KEY,
        aws_secret_access_key=S3_SECRET_KEY,
        region_name=S3_REGION,
        config=Config(signature_version='s3v4')
    )

def perform_backup():
    s3 = get_s3_client()
    if not s3:
        return False, "S3 client not configured."

    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    archive_name = f"{PREFIX}{timestamp}.tar.gz"
    
    print(f"📦 Generating safe database backup via ApexKit CLI...")
    try:
        # Backup all files, across all tenants and root
        subprocess.run(
            ["./apexkit", "backup", '--root=*', '--tenants=*', "--out", archive_name], 
            check=True,
            text=True
        )
    except subprocess.CalledProcessError as e:
        print(f"❌ CLI Backup Failed: {e}")
        return False, f"CLI Backup Failed: {e}"

    print(f"☁️  Uploading {archive_name} to bucket '{S3_BUCKET_NAME}'...")
    try:
        s3.upload_file(archive_name, S3_BUCKET_NAME, archive_name)
        print(f"✅ Upload successful! Size: {os.path.getsize(archive_name) / (1024*1024):.2f} MB")
        
        os.remove(archive_name)

        # --- PRUNE OLD BACKUPS ---
        print(f"🧹 Pruning old backups (Keeping {MAX_BACKUPS_TO_KEEP})...")
        response = s3.list_objects_v2(Bucket=S3_BUCKET_NAME, Prefix=PREFIX)
        if 'Contents' in response:
            backups = sorted(response['Contents'], key=lambda x: x['LastModified'], reverse=True)
            
            # Slice the array, grabbing everything AFTER the max count
            if len(backups) > MAX_BACKUPS_TO_KEEP:
                for old_backup in backups[MAX_BACKUPS_TO_KEEP:]:
                    old_key = old_backup['Key']
                    print(f"  🗑️ Deleting {old_key}")
                    s3.delete_object(Bucket=S3_BUCKET_NAME, Key=old_key)

        return True, f"Backup successful: {archive_name}"
    except Exception as e:
        print(f"❌ S3 Upload failed: {e}")
        return False, str(e)

def perform_restore():
    s3 = get_s3_client()
    if not s3:
        return False
    
    print(f"☁️  Scanning for latest backup in bucket '{S3_BUCKET_NAME}'...")
    try:
        response = s3.list_objects_v2(Bucket=S3_BUCKET_NAME, Prefix=PREFIX)
        
        if 'Contents' not in response or len(response['Contents']) == 0:
            print("⚠️  No backups found in bucket. Starting fresh environment.")
            return False
            
        backups = sorted(response['Contents'], key=lambda x: x['LastModified'], reverse=True)
        latest_backup_key = backups[0]['Key']

        print(f"📥 Downloading backup: {latest_backup_key}...")
        temp_download_name = "restore_candidate.tar.gz"
        s3.download_file(S3_BUCKET_NAME, latest_backup_key, temp_download_name)
        
        print(f"📦 Restoring databases via ApexKit CLI...")
        subprocess.run(
            ["./apexkit", "restore", temp_download_name, "--yes"], 
            check=True,
            text=True
        )
        
        os.remove(temp_download_name)
        print("✅ Restore successful! Files are fully up to date.")
        return True

    except Exception as e:
        print(f"⚠️  Restore failed. Error: {e}")
        return False

app = Flask(__name__)

@app.route('/backup', methods=['POST', 'GET'])
def trigger_backup():
    success, msg = perform_backup()
    if success:
        return jsonify({"status": "success", "message": msg}), 200
    else:
        return jsonify({"status": "error", "message": msg}), 500

if __name__ == '__main__':
    if len(sys.argv) > 1:
        if sys.argv[1] == '--restore':
            perform_restore()
        elif sys.argv[1] == '--serve':
            app.run(host='127.0.0.1', port=5000)
    else:
        print("Use --restore or --serve")