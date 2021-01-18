from django.db import models


class Instrument(models.Model):
    name = models.TextField(max_length="64", blank=False)


class Band(models.Model):
    name = models.TextField(max_length=128, blank=False)


class Artist(models.Model):
    name = models.TextField(max_length=128, blank=False)
    bands = models.ManyToManyField(Band)


class Album(models.Model):
    title = models.TextField(max_length=256, primary_key=True, blank=False)
    released = models.BooleanField(blank=False)
    url = models.URLField(blank=True)


class Discography(models.Model):
    """
    This represents the root storage element of music.
    For me this is OneDrive. So the storage root path is OneDrive's mount point. And the type is the string 'onedrive'
    """
    storage_root_path = models.FilePathField(blank=False)
    type = models.TextField(blank=False)


class Song(models.Model):
    title = models.TextField(max_length=256, blank=False)
    sheet_music = models.FilePathField(blank=True)
    lyrics = models.FilePathField(blank=True)
    album = models.ForeignKey(Album, on_delete=models.PROTECT)
    artists = models.ManyToManyField(Artist)
    discography = models.ForeignKey(Discography, on_delete=models.PROTECT)


class Cover(Song):
    notes = models.ImageField()
    covered_instruments = models.ManyToManyField(Instrument)
    notes_completed = models.BooleanField()


class Composition(Song):
    written_instruments = models.ManyToManyField(Instrument)
    beats_per_minute_upper = models.IntegerField()
    beats_per_minute_lower = models.IntegerField()


class Recording(models.Model):
    instruments = models.JSONField(blank=True)
    type = models.TextField(max_length=64)
    path = models.FilePathField(blank=True)
    song = models.ForeignKey(Song, blank=False, on_delete=models.PROTECT)
