# Generated by Django 3.1.5 on 2021-01-18 01:36

from django.db import migrations, models
import django.db.models.deletion


class Migration(migrations.Migration):

    initial = True

    dependencies = [
    ]

    operations = [
        migrations.CreateModel(
            name='Album',
            fields=[
                ('title', models.TextField(max_length=256, primary_key=True, serialize=False)),
                ('released', models.BooleanField()),
                ('url', models.URLField(blank=True)),
            ],
        ),
        migrations.CreateModel(
            name='Artist',
            fields=[
                ('id', models.AutoField(auto_created=True, primary_key=True, serialize=False, verbose_name='ID')),
                ('name', models.TextField(max_length=128)),
            ],
        ),
        migrations.CreateModel(
            name='Band',
            fields=[
                ('id', models.AutoField(auto_created=True, primary_key=True, serialize=False, verbose_name='ID')),
                ('name', models.TextField(max_length=128)),
            ],
        ),
        migrations.CreateModel(
            name='Discography',
            fields=[
                ('id', models.AutoField(auto_created=True, primary_key=True, serialize=False, verbose_name='ID')),
                ('storage_root_path', models.FilePathField()),
                ('type', models.TextField()),
            ],
        ),
        migrations.CreateModel(
            name='Instrument',
            fields=[
                ('id', models.AutoField(auto_created=True, primary_key=True, serialize=False, verbose_name='ID')),
                ('name', models.TextField(max_length='64')),
            ],
        ),
        migrations.CreateModel(
            name='Song',
            fields=[
                ('id', models.AutoField(auto_created=True, primary_key=True, serialize=False, verbose_name='ID')),
                ('title', models.TextField(max_length=256)),
                ('sheet_music', models.FilePathField(blank=True)),
                ('lyrics', models.FilePathField(blank=True)),
                ('album', models.ForeignKey(on_delete=django.db.models.deletion.PROTECT, to='Discography.album')),
                ('artists', models.ManyToManyField(to='Discography.Artist')),
                ('discography', models.ForeignKey(on_delete=django.db.models.deletion.PROTECT, to='Discography.discography')),
            ],
        ),
        migrations.CreateModel(
            name='Recording',
            fields=[
                ('id', models.AutoField(auto_created=True, primary_key=True, serialize=False, verbose_name='ID')),
                ('instruments', models.JSONField(blank=True)),
                ('type', models.TextField(max_length=64)),
                ('path', models.FilePathField(blank=True)),
                ('song', models.ForeignKey(on_delete=django.db.models.deletion.PROTECT, to='Discography.song')),
            ],
        ),
        migrations.AddField(
            model_name='artist',
            name='bands',
            field=models.ManyToManyField(to='Discography.Band'),
        ),
        migrations.CreateModel(
            name='Cover',
            fields=[
                ('song_ptr', models.OneToOneField(auto_created=True, on_delete=django.db.models.deletion.CASCADE, parent_link=True, primary_key=True, serialize=False, to='Discography.song')),
                ('notes', models.ImageField(upload_to='')),
                ('notes_completed', models.BooleanField()),
                ('covered_instruments', models.ManyToManyField(to='Discography.Instrument')),
            ],
            bases=('Discography.song',),
        ),
        migrations.CreateModel(
            name='Composition',
            fields=[
                ('song_ptr', models.OneToOneField(auto_created=True, on_delete=django.db.models.deletion.CASCADE, parent_link=True, primary_key=True, serialize=False, to='Discography.song')),
                ('beats_per_minute_upper', models.IntegerField()),
                ('beats_per_minute_lower', models.IntegerField()),
                ('written_instruments', models.ManyToManyField(to='Discography.Instrument')),
            ],
            bases=('Discography.song',),
        ),
    ]
